//! Three bars per agent card, and the call they add up to: keep going, compact,
//! hand off, or stop.
//!
//! The wall already shows Σ session tokens, which rises whether a session is
//! healthy or dying and so decides nothing. What it cannot show is the drag: a
//! window grown to 550k tokens costs 550k tokens of re-read before the agent has
//! thought about anything, and will cost that again next turn, and the one after.
//!
//! ```text
//!   WINDOW     how full the context is. Exact, not estimated.
//!   FATIGUE    accumulated damage, independent of size.
//!   RELEVANCE  how much of what is loaded still serves the task.
//! ```
//!
//! The pair that earns the space is WINDOW against RELEVANCE, because the two
//! full-context failures need opposite treatment and look identical on a token
//! counter: full + ballast is a cheap compaction, full + load-bearing is a
//! hand-off, and compacting the second destroys detail still in use.
//!
//! **Whole-file parse, deliberately.** An earlier design read a bounded tail to
//! keep the cost down. Measured, that was solving nothing: the biggest transcript
//! on this machine (33MB) parses in ~250ms and the entire seventeen-agent fleet
//! in 560ms — on a background thread, only when a file has grown. The tail would
//! have truncated the churn/retread baselines and dropped compaction scars older
//! than the window, buying imprecision for time nobody was spending.
//!
//! Ported from `scripts/td-agent-vitals.mjs`, which stays as the reference
//! implementation and the differential oracle — see `vitals_oracle_tests`.

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

// ─── context limits ────────────────────────────────────────────────────────
//
// The `[1m]` suffix marks the 1M-token context, and the obvious place to read it
// is the wrong one: `.message.model` says plain `claude-opus-5` on every turn of
// a session demonstrably holding 550k. Only the `cost-state` record spells the
// variant out. Reading the message field alone reports such a session as 275%
// full, clamps it to 100%, and advises compacting an agent with 450k of headroom.

const DEFAULT_LIMIT: u64 = 200_000;
const TIERS: [u64; 2] = [200_000, 1_000_000];

/// Claude Code compacts before the hard ceiling; this is what it aims at.
const AUTOCOMPACT_FLOOR: f64 = 0.92;

fn nominal_limit(id: &str) -> u64 {
    if id.contains("[1m]") {
        1_000_000
    } else if id.contains("opus")
        || id.contains("sonnet")
        || id.contains("fable")
        || id.contains("haiku")
    {
        200_000
    } else {
        0
    }
}

/// Three sources, least trusted last: the cost-state key, the message model, and
/// the transcript itself. The transcript outranks both — a window that HELD 550k
/// tokens is not a 200k window, whatever any id claims.
fn context_limit(cost_models: &[String], model: Option<&str>, peak: u64) -> u64 {
    let mut limit = cost_models
        .iter()
        .map(|k| nominal_limit(k))
        .max()
        .unwrap_or(0);
    if limit == 0 {
        limit = model.map(nominal_limit).unwrap_or(0);
    }
    if limit == 0 {
        limit = DEFAULT_LIMIT;
    }
    if peak > limit {
        limit = TIERS.iter().copied().find(|t| *t >= peak).unwrap_or(peak);
    }
    limit
}

// ─── the bar ───────────────────────────────────────────────────────────────

/// Which direction is bad. Drives the hue, never the length.
///
/// There is no `Neutral` variant, and there does not need to be: a stat that is
/// neither good nor bad renders white by carrying `charge: 0`, which is exactly
/// what WINDOW does below its calm threshold. Neutrality is a value, not a
/// category — which is also why a half-full context and a nearly-full one share
/// one tone and differ only in how far from white they have travelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Rising is worse — the bar reddens.
    Bad,
    /// Rising is better — the bar greens.
    Good,
}

/// The minimum a bar ever draws, so an empty stat still reads as a bar rather
/// than as a missing one.
pub const MIN_FILL: f32 = 0.07;

/// One stat, ready to draw.
#[derive(Debug, Clone)]
pub struct Bar {
    pub label: &'static str,
    /// 0..1 — how far the fill runs.
    pub fill: f32,
    pub tone: Tone,
    /// 0..1 — how saturated the far end of the gradient is. 0 is pure white.
    ///
    /// Separate from `fill` on purpose. A context window at 40% is FULLER than
    /// one at 7% but no more alarming, so WINDOW carries its charge on a delayed
    /// ramp while its fill tracks the honest number.
    pub charge: f32,
    /// The right-hand caption: "55%", "126.3k ballast".
    pub caption: String,
}

impl Bar {
    /// What the fill actually draws, floored so the bar is always visible.
    pub fn drawn(&self) -> f32 {
        self.fill.clamp(MIN_FILL, 1.0)
    }
}

/// Smooth 0→1 across `[lo, hi]`, flat outside it. WINDOW's charge rides this so
/// the bar stays white through the comfortable range and only reddens as the
/// ceiling comes into view.
fn smoothstep(lo: f32, hi: f32, v: f32) -> f32 {
    if hi <= lo {
        return if v >= hi { 1.0 } else { 0.0 };
    }
    let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Where WINDOW starts to matter, and where it is as loud as it gets.
const WINDOW_CALM: f32 = 0.60;
const WINDOW_ALARM: f32 = 0.98;

// ─── the call ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Call {
    Run,
    Watch,
    Compact,
    HandOff,
    Stop,
}

impl Call {
    pub fn label(self) -> &'static str {
        match self {
            Call::Run => "RUN",
            Call::Watch => "WATCH",
            Call::Compact => "COMPACT",
            Call::HandOff => "HAND OFF",
            Call::Stop => "STOP",
        }
    }

    /// Does this agent want a decision from a human?
    ///
    /// Only STOP and HAND OFF. COMPACT is a thing to do, not a thing to decide,
    /// and a wall that flags every healthy agent teaches you to read past the
    /// flags — which is the failure the whole surface exists to avoid.
    pub fn needs_you(self) -> bool {
        matches!(self, Call::Stop | Call::HandOff)
    }
}

// ─── the result ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Vitals {
    pub window: Bar,
    pub fatigue: Bar,
    pub relevance: Bar,
    pub call: Call,
    /// One line of evidence for the call, for the tooltip and the CLI.
    pub why: String,
    /// `OPUS`, `FABLE`, `GPT` — name only, no version.
    pub model: Option<String>,
    /// `HIGH`, `MAX` — verbatim from the transcript, never invented.
    pub effort: Option<String>,
    pub tokens: u64,
    pub limit: u64,
    pub turns_left: Option<u64>,
    pub ballast: u64,
    /// Size and mtime of the transcript this was computed from — the cache key.
    pub stamp: (u64, u64),
}

impl Vitals {
    /// The single persistent label: `OPUS · MAX`.
    ///
    /// Replaces the old MODEL/EFFORT pair, which never once showed a model —
    /// `parse_model` read a `--model` flag nobody passes, fell through to the
    /// program name, and printed `MODEL CLAUDE` on every card in the fleet.
    pub fn chip(&self) -> Option<String> {
        let m = self.model.as_deref()?;
        Some(match self.effort.as_deref() {
            Some(e) if !e.is_empty() => format!("{m} \u{b7} {e}"),
            _ => m.to_string(),
        })
    }

    pub fn bars(&self) -> [&Bar; 3] {
        [&self.window, &self.fatigue, &self.relevance]
    }
}

/// Model family, no version — `claude-opus-5` and `claude-opus-4-8` are both
/// OPUS, which is what was asked for and what fits the chip.
pub fn model_name(raw: &str) -> Option<String> {
    let id = raw.trim().to_ascii_lowercase();
    if id.is_empty() || id == "<synthetic>" {
        return None;
    }
    for (needle, name) in [
        ("opus", "OPUS"),
        ("fable", "FABLE"),
        ("sonnet", "SONNET"),
        ("haiku", "HAIKU"),
        ("gpt", "GPT"),
        ("codex", "CODEX"),
        ("gemini", "GEMINI"),
    ] {
        if id.contains(needle) {
            return Some(name.to_string());
        }
    }
    // An unknown provider still deserves a name: take the leading alphabetic
    // run, which is the family in every id shape seen so far.
    let head: String = id.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    (!head.is_empty()).then(|| head.to_uppercase())
}

// ─── the turn model ────────────────────────────────────────────────────────

struct Turn {
    ts: i64,
    ctx: u64,
    created: u64,
    out: u64,
}

struct CallRec {
    paths: Vec<String>,
    key: String,
    turn_index: usize,
    /// The assistant record's own timestamp. Distinct from its turn's: an
    /// assistant message carrying no `usage` block pushes no turn, so
    /// `turn_index` points at the PREVIOUS one, and using that to find the
    /// working set moved the focus boundary and shifted RELEVANCE by six points.
    ts: i64,
    /// Set once the matching tool_result lands.
    result: Option<usize>,
}

struct ResultRec {
    error: bool,
    chars: usize,
    tokens: u64,
}

struct Compaction {
    post: u64,
    dropped: u64,
    call_index: usize,
}

#[derive(Default)]
struct Transcript {
    turns: Vec<Turn>,
    calls: Vec<CallRec>,
    results: Vec<ResultRec>,
    compactions: Vec<Compaction>,
    prompt_ts: Vec<i64>,
    cost_models: Vec<String>,
    model: Option<String>,
    effort: Option<String>,
    started: Option<i64>,
    ended: Option<i64>,
}

/// Unix **milliseconds**. Seconds would be the obvious choice and it is wrong:
/// turns land under a second apart, and at second resolution their gap rounds to
/// zero, drops out of the latency sample, and shifts FATIGUE by a point or three
/// against the reference implementation. Caught by the oracle diff, not by any
/// unit test — both implementations were self-consistent.
fn parse_ts(v: Option<&str>) -> Option<i64> {
    // `2026-09-02T18:45:11.840Z` — fixed width, so field slicing beats a parser
    // dependency the crate does not have.
    let s = v?;
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[10] != b'T' {
        return None;
    }
    let num = |a: usize, z: usize| -> Option<i64> { s.get(a..z)?.parse::<i64>().ok() };
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    // Days since the epoch, civil-calendar (Howard Hinnant's algorithm).
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let millis = s
        .get(20..23)
        .filter(|f| f.bytes().all(|c| c.is_ascii_digit()))
        .and_then(|f| f.parse::<i64>().ok())
        .unwrap_or(0);
    Some((days * 86_400 + h * 3_600 + mi * 60 + sec) * 1000 + millis)
}

/// Is this bare token a file rather than, say, a dotted module name? Without the
/// extension list `python3 -m http.server` contributes `http.server` as a path,
/// and one junk entry in the focus set scores unrelated work as on-topic.
fn looks_like_path(s: &str) -> bool {
    if s.contains('/') {
        return true;
    }
    let Some(dot) = s.rfind('.') else {
        return false;
    };
    if dot == 0 || dot + 1 >= s.len() {
        return false;
    }
    const EXTS: &[&str] = &[
        "rs", "ts", "tsx", "js", "mjs", "cjs", "jsx", "py", "rb", "go", "java", "c", "h", "cpp",
        "hpp", "sh", "bash", "zsh", "fish", "md", "mdx", "txt", "json", "jsonl", "toml", "yaml",
        "yml", "html", "css", "scss", "lock", "sql", "xml", "svg", "png", "webp", "conf", "ini",
        "service", "socket", "qml", "lua", "vim", "el", "nix",
    ];
    let ext = s[dot + 1..].to_ascii_lowercase();
    EXTS.contains(&ext.as_str())
}

fn normalise(raw: &str) -> Option<String> {
    let s = raw
        .trim()
        .trim_end_matches([')', ']', '}', ',', '.', ':', ';', '\'', '"', '`']);
    if s.is_empty() || s == "." || s == ".." {
        return None;
    }
    // `~/Work/x.rs` and `/home/parker/Work/x.rs` are ONE file, and a path's
    // identity is what supersession and the focus set are keyed on. Leaving the
    // tilde unexpanded split one file into two, and ~4.4k tokens that should
    // have read as superseded copies scored as live context instead.
    if let Some(rest) = s.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        return Some(format!("{home}/{rest}"));
    }
    let s = s.strip_prefix("./").unwrap_or(s);
    Some(s.to_string())
}

fn add_path(out: &mut Vec<String>, raw: &str) {
    if let Some(s) = normalise(raw) {
        if looks_like_path(&s) && !out.contains(&s) {
            out.push(s);
        }
    }
}

/// Paths out of a shell command as well as off a `file_path` argument. Half the
/// calls in a lean-ctx session are a command with the path buried in an
/// argument, so a `file_path` lookup alone sees nothing and scores the whole
/// session as unfocused.
///
/// A faithful port of the reference implementation's regex, and the details
/// matter: a run of path characters counts only where it FOLLOWS a delimiter or
/// starts the string. Splitting on a looser set instead lifted `bar/baz.rs` out
/// of `foo:bar/baz.rs` — a path the reference never sees — and every extra path
/// shifts supersession and the focus set with it.
fn is_path_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '@' | '+' | '-' | '/' | '~')
}

/// Cut a run of path characters down to the shape a path can actually have:
/// one optional root prefix, then non-empty segments joined by single slashes.
///
/// Two things fall out of this, both of which showed up as real disagreements.
/// A trailing slash is never part of the name — `app/src/` and `app/src` are one
/// directory, and treating them as two split a focus entry in half. And a path
/// cannot cross an empty segment, so the jq expression `.conclusion//.state`
/// yields `.conclusion` (no slash, no known extension → not a path at all)
/// rather than being swallowed whole as one.
fn path_shaped(tok: &str) -> String {
    let (prefix, rest) = if let Some(r) = tok.strip_prefix("~/") {
        ("~/", r)
    } else if let Some(r) = tok.strip_prefix("../") {
        ("../", r)
    } else if let Some(r) = tok.strip_prefix("./") {
        ("./", r)
    } else if let Some(r) = tok.strip_prefix('/') {
        ("/", r)
    } else {
        ("", tok)
    };
    let mut segs = Vec::new();
    for s in rest.split('/') {
        if s.is_empty() {
            break;
        }
        segs.push(s);
    }
    if segs.is_empty() {
        return String::new(); // a bare `/` names nothing
    }
    format!("{prefix}{}", segs.join("/"))
}

fn scan_paths(prose: &str, out: &mut Vec<String>) {
    let chars: Vec<char> = prose.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !is_path_char(chars[i]) {
            i += 1;
            continue;
        }
        let opens = i == 0
            || matches!(
                chars[i - 1],
                ' ' | '\t' | '\n' | '\r' | '"' | '\'' | '`' | '=' | '(' | ','
            );
        let start = i;
        while i < chars.len() && is_path_char(chars[i]) {
            i += 1;
        }
        if opens {
            let tok: String = chars[start..i].iter().collect();
            let shaped = path_shaped(&tok);
            if !shaped.is_empty() {
                add_path(out, &shaped);
            }
        }
    }
}

/// Two different things are wanted from a call. `paths` is what it TOUCHED, and
/// only a later call on the same path supersedes an earlier one. `key` is what
/// it WAS, so churn can tell `git status` run eight times from eight different
/// commands. Keying both on the tool name made every shell call in a session
/// supersede every other one and scored a healthy transcript at 8% relevant.
fn targets_of(name: &str, input: &Value) -> (Vec<String>, String) {
    let mut paths = Vec::new();
    for k in ["file_path", "path", "notebook_path", "filePath"] {
        if let Some(s) = input.get(k).and_then(Value::as_str) {
            add_path(&mut paths, s);
        }
    }
    if let Some(arr) = input.get("paths").and_then(Value::as_array) {
        for v in arr {
            if let Some(s) = v.as_str() {
                add_path(&mut paths, s);
            }
        }
    }
    // Paths are scanned across every prose field a tool might carry.
    let mut prose = String::new();
    for k in ["command", "pattern", "query", "task", "description"] {
        if let Some(s) = input.get(k).and_then(Value::as_str) {
            prose.push(' ');
            prose.push_str(s);
        }
    }
    scan_paths(&prose, &mut paths);

    let key = if paths.is_empty() {
        // A call naming no path is identified by what it did — the command
        // itself, collapsed, so repetition is visible and variety is not.
        //
        // The signature reads the FIRST field present, and `description` is not
        // among them. Concatenating all five instead moved churn by a point and
        // relevance by six: a Bash call carries both `command` and
        // `description`, so the joined key changed whenever the description did,
        // splitting repeats of one command into separate keys.
        let sig = ["command", "pattern", "query", "task"]
            .iter()
            .find_map(|k| input.get(*k).and_then(Value::as_str))
            .unwrap_or("");
        let collapsed: String = sig.split_whitespace().collect::<Vec<_>>().join(" ");
        format!(
            "{name}::{}",
            collapsed.chars().take(120).collect::<String>()
        )
    } else {
        let mut sorted = paths.clone();
        sorted.sort();
        sorted.join("|")
    };
    (paths, key)
}

fn parent_dir(p: &str) -> Option<&str> {
    let d = p.rfind('/')?;
    let parent = &p[..d];
    (!parent.is_empty()).then_some(parent)
}

fn parse_transcript(body: &str) -> Transcript {
    let mut t = Transcript::default();
    let mut by_id: HashMap<String, usize> = HashMap::new();

    for line in body.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            // A transcript appended to while we read it hands us a torn line.
            // Skipping costs one turn of freshness; failing costs the reading.
            continue;
        };
        let ts = parse_ts(v.get("timestamp").and_then(Value::as_str));
        if let Some(n) = ts {
            t.started = Some(t.started.map_or(n, |s: i64| s.min(n)));
            t.ended = Some(t.ended.map_or(n, |e: i64| e.max(n)));
        }
        let kind = v.get("type").and_then(Value::as_str).unwrap_or("");

        if kind == "cost-state" {
            if let Some(m) = v.get("modelUsage").and_then(Value::as_object) {
                t.cost_models = m.keys().cloned().collect();
            }
            continue;
        }

        if kind == "system" && v.get("subtype").and_then(Value::as_str) == Some("compact_boundary")
        {
            let m = v.get("compactMetadata");
            t.compactions.push(Compaction {
                post: m
                    .and_then(|m| m.get("postTokens"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                dropped: m
                    .and_then(|m| m.get("cumulativeDroppedTokens"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                call_index: t.calls.len(),
            });
            continue;
        }

        if kind == "assistant" {
            let Some(msg) = v.get("message") else {
                continue;
            };
            if let Some(m) = msg.get("model").and_then(Value::as_str) {
                if m != "<synthetic>" {
                    t.model = Some(m.to_string());
                }
            }
            // Effort rides the RECORD, not the message — and it is the only
            // honest source: `hud::extract_effort` scrapes the status line and
            // knows only high/medium/low, so it cannot emit `max` at all.
            if let Some(e) = v.get("effort").and_then(Value::as_str) {
                if !e.is_empty() {
                    t.effort = Some(e.to_string());
                }
            }
            if let Some(u) = msg.get("usage") {
                let g = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
                t.turns.push(Turn {
                    ts: ts.unwrap_or(0),
                    ctx: g("input_tokens")
                        + g("cache_creation_input_tokens")
                        + g("cache_read_input_tokens"),
                    created: g("cache_creation_input_tokens"),
                    out: g("output_tokens"),
                });
            }
            let turn_index = t.turns.len().saturating_sub(1);
            if let Some(content) = msg.get("content").and_then(Value::as_array) {
                for c in content {
                    if c.get("type").and_then(Value::as_str) != Some("tool_use") {
                        continue;
                    }
                    let name = c.get("name").and_then(Value::as_str).unwrap_or("");
                    let empty = Value::Null;
                    let (paths, key) = targets_of(name, c.get("input").unwrap_or(&empty));
                    if let Some(id) = c.get("id").and_then(Value::as_str) {
                        by_id.insert(id.to_string(), t.calls.len());
                    }
                    t.calls.push(CallRec {
                        paths,
                        key,
                        turn_index,
                        ts: ts.unwrap_or(0),
                        result: None,
                    });
                }
            }
            continue;
        }

        if kind == "user" {
            let Some(msg) = v.get("message") else {
                continue;
            };
            let is_summary = v.get("isCompactSummary").and_then(Value::as_bool) == Some(true);
            match msg.get("content") {
                Some(Value::String(_)) => {
                    if !is_summary {
                        if let Some(n) = ts {
                            t.prompt_ts.push(n);
                        }
                    }
                }
                Some(Value::Array(items)) => {
                    let tool_results: Vec<&Value> = items
                        .iter()
                        .filter(|c| c.get("type").and_then(Value::as_str) == Some("tool_result"))
                        .collect();
                    if tool_results.is_empty() {
                        let has_text = items
                            .iter()
                            .any(|c| c.get("type").and_then(Value::as_str) == Some("text"));
                        if has_text && !is_summary {
                            if let Some(n) = ts {
                                t.prompt_ts.push(n);
                            }
                        }
                        continue;
                    }
                    for tr in tool_results {
                        let body = match tr.get("content") {
                            Some(Value::String(s)) => s.len(),
                            Some(other) => other.to_string().len(),
                            None => 0,
                        };
                        let idx = t.results.len();
                        t.results.push(ResultRec {
                            error: tr.get("is_error").and_then(Value::as_bool) == Some(true),
                            chars: body,
                            tokens: 0,
                        });
                        if let Some(ci) = tr
                            .get("tool_use_id")
                            .and_then(Value::as_str)
                            .and_then(|id| by_id.get(id))
                            .copied()
                        {
                            t.calls[ci].result = Some(idx);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    t
}

// ─── what each result cost ─────────────────────────────────────────────────
//
// Two traps, both of which produced numbers that could not be true and printed
// anyway.
//
// A turn can issue several tool calls at once and they share one cache write, so
// charging each the whole turn's creation counts it many times. The budget is
// split across the turn's results in proportion to their size.
//
// Worse, and not fixed by splitting: `cache_creation` covers everything new
// since the last write, and a tool result is not the only thing arriving between
// turns — a pasted message, hook output, injected reminders and a re-sent system
// block all land in the same number. Measured on a real session, eleven errored
// results totalling 3,800 characters were charged 197,400 tokens. The exact
// figure was exactly right about the turn and wildly wrong about the result.
//
// So the estimate leads and the measurement corroborates, used only where the two
// agree to within a factor of a couple.

const CHARS_PER_TOKEN: f64 = 4.0;

fn attribute_tokens(t: &mut Transcript) {
    let mut by_turn: HashMap<usize, Vec<usize>> = HashMap::new();
    for (ci, c) in t.calls.iter().enumerate() {
        if c.result.is_some() {
            by_turn.entry(c.turn_index).or_default().push(ci);
        }
    }

    for (ti, group) in by_turn {
        let chars: usize = group
            .iter()
            .map(|&ci| t.results[t.calls[ci].result.unwrap()].chars)
            .sum();
        let chars = chars.max(1);
        let estimate = chars as f64 / CHARS_PER_TOKEN;

        let mut budget = None;
        if let (Some(cur), Some(next)) = (t.turns.get(ti), t.turns.get(ti + 1)) {
            if next.created > 0 {
                let exact = next.created as f64 - cur.out as f64;
                // The corroboration band. Below it the cache did not capture
                // these results; above it the turn carried something else too.
                if exact > estimate * 0.4 && exact < estimate * 2.5 {
                    budget = Some(exact);
                }
            }
        }

        for &ci in &group {
            let ri = t.calls[ci].result.unwrap();
            let share = t.results[ri].chars as f64 / chars as f64;
            t.results[ri].tokens = match budget {
                Some(b) => (b * share).round().max(1.0) as u64,
                None => (t.results[ri].chars as f64 / CHARS_PER_TOKEN)
                    .ceil()
                    .max(1.0) as u64,
            };
        }
    }

    for r in t.results.iter_mut() {
        if r.tokens == 0 {
            r.tokens = (r.chars as f64 / CHARS_PER_TOKEN).ceil().max(1.0) as u64;
        }
    }
}

// ─── the three measures ────────────────────────────────────────────────────

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = v.len() / 2;
    if v.len() % 2 == 1 {
        v[mid]
    } else {
        (v[mid - 1] + v[mid]) / 2.0
    }
}

struct WindowStat {
    tokens: u64,
    limit: u64,
    fill: f64,
    growth: u64,
    turns_left: Option<u64>,
}

fn window_stat(t: &Transcript) -> WindowStat {
    let peak = t.turns.iter().map(|x| x.ctx).max().unwrap_or(0);
    let limit = context_limit(&t.cost_models, t.model.as_deref(), peak);
    let tokens = t.turns.last().map(|x| x.ctx).unwrap_or(0);

    // Growth over the recent tail only. An early-session average is meaningless:
    // the first turns load a system prompt and a working set, which never
    // happens again. A negative delta is a compaction, not growth.
    let tail: Vec<&Turn> = t.turns.iter().rev().take(12).collect();
    let mut deltas: Vec<f64> = tail
        .windows(2)
        .filter_map(|w| {
            let d = w[0].ctx as i64 - w[1].ctx as i64;
            (d > 0).then_some(d as f64)
        })
        .collect();
    let growth = median(&mut deltas).round() as u64;

    let ceiling = (limit as f64 * AUTOCOMPACT_FLOOR) as u64;
    let turns_left = (growth > 0).then(|| ceiling.saturating_sub(tokens) / growth);

    WindowStat {
        tokens,
        limit,
        fill: if limit > 0 {
            tokens as f64 / limit as f64
        } else {
            0.0
        },
        growth,
        turns_left,
    }
}

struct FatigueStat {
    score: f64,
    reason: String,
    /// scars, errors, churn, latency, retread, age — carried so the oracle diff
    /// can compare components rather than a single blended number.
    parts: [f64; 6],
}

/// Deliberately NOT a restatement of context size — the wall already has that
/// bar, and two bars that move together are one bar. Fatigue is the damage a long
/// session accumulates that a fresh one would not have.
fn fatigue_stat(t: &Transcript, limit: u64) -> FatigueStat {
    // Scars. A compaction trades detail for room, and the agent afterwards works
    // from its own summary. Two is a different animal from none.
    let dropped: u64 = t.compactions.iter().map(|c| c.dropped).sum();
    let scars = (t.compactions.len() as f64 * 0.34 + (dropped as f64 / limit.max(1) as f64) * 0.25)
        .clamp(0.0, 1.0);

    // Errors, as a TREND. The level is a property of the task — exploratory shell
    // work errors more than file editing, and neither is fatigue. Two errors that
    // both landed late is also not a trend, however dramatic the ratio: a session
    // with 2 errors in 81 calls read 100 for "errors climbing".
    let errs: Vec<f64> = t
        .results
        .iter()
        .map(|r| if r.error { 1.0 } else { 0.0 })
        .collect();
    let err_count = errs.iter().filter(|e| **e > 0.0).count();
    let cut = (errs.len() as f64 * 0.6) as usize;
    let mean = |s: &[f64]| {
        if s.is_empty() {
            0.0
        } else {
            s.iter().sum::<f64>() / s.len() as f64
        }
    };
    let errors = if errs.len() < 20 || err_count < 4 {
        0.0
    } else {
        (mean(&errs[cut..]) / mean(&errs[..cut]).max(0.03) - 1.0).clamp(0.0, 1.0)
    };
    let error_rate = mean(&errs);

    // Churn. The third time, not the second — doing a thing twice is how
    // iteration looks, and counting that pinned every working session at 100.
    let half = t.calls.len() / 2;
    let mut seen_keys: HashMap<&str, u32> = HashMap::new();
    let mut repeats = 0u32;
    for c in &t.calls[half..] {
        let n = seen_keys.entry(c.key.as_str()).or_insert(0);
        *n += 1;
        if *n > 2 {
            repeats += 1;
        }
    }
    let churn = if t.calls.len() > half {
        ((repeats as f64 / (t.calls.len() - half) as f64) * 1.2).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // Latency drift. Not raw duration — a long turn may be a big job. Time per
    // 1k output tokens, recent against early. Halved so the bar reads full at
    // three times slower: twice as long per token is a session that moved from
    // reading files to running builds, which is a change of work.
    let pairs: Vec<f64> = t
        .turns
        .windows(2)
        .filter_map(|w| {
            let dt = w[1].ts - w[0].ts;
            (dt > 0 && dt < 900_000 && w[1].out > 40)
                .then(|| (dt as f64 / w[1].out as f64) * 1000.0)
        })
        .collect();
    let latency = if pairs.len() < 12 {
        0.0
    } else {
        let c = (pairs.len() as f64 * 0.6) as usize;
        let early = median(&mut pairs[..c].to_vec());
        let recent = median(&mut pairs[c..].to_vec());
        if early <= 0.0 {
            0.0
        } else {
            ((recent / early - 1.0) / 2.0).clamp(0.0, 1.0)
        }
    };

    // Retread. Only a SUBSTANTIAL re-fetch counts: re-reading a file you are
    // editing is how editing works, and with lean-ctx a repeat read costs a diff.
    const RETREAD_FLOOR: u64 = 2000;
    let mut seen_paths: HashSet<&str> = HashSet::new();
    let (mut retread_tokens, mut total_tokens) = (0u64, 0u64);
    for (i, c) in t.calls.iter().enumerate() {
        if let Some(ri) = c.result {
            if i >= half {
                let tk = t.results[ri].tokens;
                total_tokens += tk;
                if tk > RETREAD_FLOOR
                    && !c.paths.is_empty()
                    && c.paths.iter().all(|p| seen_paths.contains(p.as_str()))
                {
                    retread_tokens += tk;
                }
            }
        }
        for p in &c.paths {
            seen_paths.insert(p.as_str());
        }
    }
    let retread = if total_tokens > 0 {
        ((retread_tokens as f64 / total_tokens as f64) * 1.5).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // Age. Weakest, and last for that reason — a clean six-hour session is not
    // tired. It is here because wall-clock does eventually count.
    let hours = match (t.started, t.ended) {
        (Some(a), Some(b)) if b > a => (b - a) as f64 / 3_600_000.0,
        _ => 0.0,
    };
    let age = (hours / 8.0).clamp(0.0, 1.0);

    let parts = [
        ("scars", scars, 0.28),
        ("errors", errors, 0.18),
        ("churn", churn, 0.18),
        ("latency", latency, 0.14),
        ("retread", retread, 0.12),
        ("age", age, 0.10),
    ];
    let score: f64 = parts.iter().map(|(_, v, w)| v * w).sum();

    let top = parts
        .iter()
        .max_by(|a, b| {
            (a.1 * a.2)
                .partial_cmp(&(b.1 * b.2))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(n, _, _)| *n)
        .unwrap_or("age");
    let reason = match top {
        "scars" => format!(
            "{} compaction{}, {} dropped",
            t.compactions.len(),
            if t.compactions.len() == 1 { "" } else { "s" },
            fmt_tokens(dropped)
        ),
        "errors" => format!(
            "tool errors climbing ({}% of calls)",
            (error_rate * 100.0).round()
        ),
        "churn" => "repeating calls it has already made".into(),
        "latency" => "turns slowing per token produced".into(),
        "retread" => "re-reading what it already had".into(),
        _ => format!("{hours:.1}h on the clock"),
    };

    FatigueStat {
        score,
        reason,
        parts: [scars, errors, churn, latency, retread, age],
    }
}

struct RelevanceStat {
    score: f64,
    ballast: u64,
    /// live, stale, superseded, errored, summary, resident-call-count — carried
    /// so an oracle disagreement is localised to a bucket rather than guessed at.
    buckets: [f64; 6],
}

/// What fraction of the loaded context still serves the task in front of the
/// agent — that is, what a fresh agent picking it up would load again. A PROXY,
/// and named as one.
fn relevance_stat(t: &Transcript, win: &WindowStat) -> RelevanceStat {
    // The working set: what has been touched since the human last spoke. A prompt
    // is the cleanest topic boundary a transcript has.
    let last_prompt = t.prompt_ts.last().copied();
    let recent: Vec<&CallRec> = match last_prompt {
        Some(p) => {
            let from = t
                .calls
                .iter()
                .position(|c| c.ts >= p)
                .unwrap_or(t.calls.len());
            let v: Vec<&CallRec> = t.calls[from..].iter().collect();
            if v.len() < 12 {
                t.calls.iter().rev().take(12).rev().collect()
            } else {
                v
            }
        }
        None => t.calls.iter().rev().take(12).rev().collect(),
    };
    let mut focus: HashSet<&str> = HashSet::new();
    for c in &recent {
        for p in &c.paths {
            focus.insert(p.as_str());
            if let Some(d) = parent_dir(p) {
                focus.insert(d);
            }
        }
        if c.paths.is_empty() {
            focus.insert(c.key.as_str());
        }
    }

    // A FILE read again later is superseded by that later copy. This holds for
    // paths and nothing else: two shell commands are not two copies of one thing,
    // and treating them as such condemns most of a healthy session.
    let mut last_read_of: HashMap<&str, usize> = HashMap::new();
    for (i, c) in t.calls.iter().enumerate() {
        for p in &c.paths {
            last_read_of.insert(p.as_str(), i);
        }
    }

    let last_compact = t
        .compactions
        .last()
        .map(|c| c.call_index as isize)
        .unwrap_or(-1);
    let summary = t.compactions.last().map(|c| c.post).unwrap_or(0);

    // Which results are still resident. A transcript is the whole history; the
    // window is its tail. Summing every result a 621-turn session ever loaded gave
    // 1.6M tokens of "context" inside a 456k window — a denominator over
    // everything that was ever true rather than over what is loaded now.
    let mut resident: Vec<usize> = Vec::new();
    let mut acc = summary;
    for i in (0..t.calls.len()).rev() {
        if (i as isize) < last_compact {
            break; // dropped at the boundary; the summary stands for it
        }
        let Some(ri) = t.calls[i].result else {
            continue;
        };
        resident.push(i);
        acc += t.results[ri].tokens;
        if acc >= win.tokens {
            break;
        }
    }

    const STALE_CREDIT: f64 = 0.35;
    let (mut live, mut stale) = (summary as f64, 0.0f64);
    let (mut sup, mut err) = (0.0f64, 0.0f64);
    let mut total = summary as f64;
    for &i in &resident {
        let c = &t.calls[i];
        let r = &t.results[c.result.unwrap()];
        let tk = r.tokens as f64;
        total += tk;
        let superseded = !c.paths.is_empty()
            && c.paths
                .iter()
                .all(|p| last_read_of.get(p.as_str()) != Some(&i));
        if r.error {
            err += tk;
            continue; // dead weight, counted in the denominator only
        }
        if superseded {
            sup += tk;
            continue;
        }
        let on_topic = c.paths.iter().any(|p| {
            focus.contains(p.as_str()) || parent_dir(p).is_some_and(|d| focus.contains(d))
        }) || (c.paths.is_empty() && focus.contains(c.key.as_str()));
        if on_topic {
            live += tk;
        } else {
            stale += tk;
        }
    }

    let useful = live + STALE_CREDIT * stale;
    let score = if total > 0.0 { useful / total } else { 1.0 };
    RelevanceStat {
        score: score.clamp(0.0, 1.0),
        // Tokens the window carries that a fresh agent on this task would not.
        ballast: ((1.0 - score) * win.tokens as f64).round() as u64,
        buckets: [live, stale, sup, err, summary as f64, resident.len() as f64],
    }
}

// ─── the call ──────────────────────────────────────────────────────────────

fn verdict(win: &WindowStat, fat: &FatigueStat, rel: &RelevanceStat) -> (Call, String) {
    let fill = win.fill;
    let r = rel.score;
    let pc = |x: f64| format!("{}%", (x * 100.0).round());

    if fat.score >= 0.70 {
        return (
            Call::Stop,
            format!(
                "{}. A fresh session starts cleaner than this one continues.",
                fat.reason
            ),
        );
    }
    // The whole point of the pair: these two look identical on a token counter
    // and need opposite treatment.
    if fill >= 0.85 && r >= 0.65 {
        return (
            Call::HandOff,
            format!(
                "{} full and {} of it load-bearing — compaction throws away detail still in use.",
                pc(fill),
                pc(r)
            ),
        );
    }
    if fill >= 0.85 {
        return (
            Call::Compact,
            format!(
                "{} full, {} of it ballast. Compaction is cheap here.",
                pc(fill),
                fmt_tokens(rel.ballast)
            ),
        );
    }
    if fill >= 0.60 && r < 0.45 {
        return (
            Call::Compact,
            format!(
                "only {} still serves the task — {} re-read every turn for nothing.",
                pc(r),
                fmt_tokens(rel.ballast)
            ),
        );
    }
    // Ballast in absolute tokens, scaled to the window. A 1M context at 55% reads
    // comfortable and can still carry a quarter-million tokens paid for on every
    // turn; the ratio hides that, the count does not. A flat threshold judged a
    // 1M window by a 200k yardstick and fired on nearly every long session.
    let floor = (120_000f64).max(win.limit as f64 * 0.25) as u64;
    if rel.ballast >= floor {
        return (
            Call::Compact,
            format!(
                "{} of ballast re-read every turn — {} relevant at {} full.",
                fmt_tokens(rel.ballast),
                pc(r),
                pc(fill)
            ),
        );
    }
    if win.turns_left.is_some_and(|n| n <= 10) {
        return (
            Call::Watch,
            format!(
                "about {} turns of headroom at {}/turn.",
                win.turns_left.unwrap(),
                fmt_tokens(win.growth)
            ),
        );
    }
    if fill >= 0.60 || fat.score >= 0.45 {
        return (
            Call::Watch,
            format!(
                "{} full, fatigue {}. Nothing wrong yet.",
                pc(fill),
                (fat.score * 100.0).round()
            ),
        );
    }
    let room = win
        .turns_left
        .map(|n| format!("~{n} turns of headroom"))
        .unwrap_or_else(|| "room to spare".into());
    if r < 0.50 {
        return (
            Call::Run,
            format!(
                "{} full with {room}. Only {} relevant, but there is room to carry it.",
                pc(fill),
                pc(r)
            ),
        );
    }
    (
        Call::Run,
        format!("{} full, {} relevant, {room}.", pc(fill), pc(r)),
    )
}

/// `1.2M`, `59.0k`, `412` — matching `hud::fmt_tokens` so the card reads as one
/// surface.
fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}

// ─── the entry point ───────────────────────────────────────────────────────

/// Compute from a transcript's text. Split from the file read so the tests can
/// build a transcript with a known answer.
pub fn from_body(body: &str, stamp: (u64, u64)) -> Option<Vitals> {
    let mut t = parse_transcript(body);
    if t.turns.is_empty() {
        return None;
    }
    attribute_tokens(&mut t);

    let win = window_stat(&t);
    let fat = fatigue_stat(&t, win.limit);
    let rel = relevance_stat(&t, &win);
    let (call, why) = verdict(&win, &fat, &rel);

    Some(Vitals {
        window: Bar {
            label: "WINDOW",
            fill: win.fill as f32,
            tone: Tone::Bad,
            // Neutral through the comfortable range: a half-full window is fine,
            // and a bar that reddens at 40% is crying wolf.
            charge: smoothstep(WINDOW_CALM, WINDOW_ALARM, win.fill as f32),
            caption: format!("{}%", (win.fill * 100.0).round()),
        },
        fatigue: Bar {
            label: "FATIGUE",
            fill: fat.score as f32,
            tone: Tone::Bad,
            charge: fat.score as f32,
            caption: format!("{}%", (fat.score * 100.0).round()),
        },
        relevance: Bar {
            label: "RELEVANCE",
            fill: rel.score as f32,
            tone: Tone::Good,
            charge: rel.score as f32,
            caption: format!("{}%", (rel.score * 100.0).round()),
        },
        call,
        why,
        model: t.model.as_deref().and_then(model_name),
        effort: t.effort.as_deref().map(|e| e.to_uppercase()),
        tokens: win.tokens,
        limit: win.limit,
        turns_left: win.turns_left,
        ballast: rel.ballast,
        stamp,
    })
}

/// Read and measure one transcript. `None` for a file that is missing, empty, or
/// has not yet produced an assistant turn — a live agent with nothing to measure
/// draws no bars rather than three empty ones.
pub fn read(path: &Path) -> Option<Vitals> {
    let md = std::fs::metadata(path).ok()?;
    let stamp = (
        md.len(),
        md.modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0),
    );
    let body = std::fs::read_to_string(path).ok()?;
    from_body(&body, stamp)
}

// ─── binding a pane to its OWN transcript ──────────────────────────────────
//
// `session::claude_transcript` answers per pane, and with no `--resume` to go on
// it falls through to `newest_jsonl` — newest by mtime, in that directory. Two
// bare `claude` panes sharing a cwd therefore get the SAME file, which is
// terminal-delight#272: observed twice in one snapshot, once putting a pane
// titled "Agent-playhouse logo terminal pane update" onto the transcript whose
// own `ai-title` reads "Sticky note pin notification".
//
// A wrong tool glyph is cosmetic. A wrong FATIGUE bar is a shutdown decision
// taken on another agent's evidence, so the wall resolves its fleet in one pass
// with each transcript claimable exactly once.
//
// mtime is also the wrong signal even for a single pane. Claude Code writes
// bookkeeping records back into transcripts a conversation has already left, so
// an abandoned session's file can be newer on disk than the live one's — the
// last RECORD is what says where an agent is speaking.

/// Seconds between a process starting and its first record. Measured at 3s and
/// 2s on the two addev agents; 90 is slack, not a guess at the real spread.
const BIRTH_TOLERANCE: i64 = 90;

/// A transcript that some pane might own.
#[derive(Debug, Clone)]
pub struct Cand {
    pub id: String,
    /// First record's timestamp — when the conversation began.
    pub began: Option<i64>,
    /// Last record's timestamp — when it was last spoken into.
    pub spoke: Option<i64>,
}

/// One agent pane, with the candidates from its own project directory.
#[derive(Debug, Clone)]
pub struct FleetPane {
    /// The agent process, not the pane's shell.
    pub pid: u32,
    /// A session id the agent itself named on its command line. The only
    /// source that outranks forensics.
    pub declared: Option<String>,
    pub started_at: Option<i64>,
    /// Indices into the candidate slice.
    pub cands: Vec<usize>,
}

/// Assign each pane a transcript, at most one pane per transcript. Pure, so the
/// two real collisions from #272 can be replayed as tests.
pub fn assign(panes: &[FleetPane], cands: &[Cand]) -> HashMap<u32, usize> {
    let mut out: HashMap<u32, usize> = HashMap::new();
    let mut claimed: HashSet<usize> = HashSet::new();

    // Declared wins outright. An agent naming its own session is not a guess.
    for p in panes {
        let Some(id) = p.declared.as_deref() else {
            continue;
        };
        if let Some(&ci) = p.cands.iter().find(|&&ci| cands[ci].id == id) {
            if claimed.insert(ci) {
                out.insert(p.pid, ci);
            }
        }
    }

    // Birth. A bare `claude` opens its conversation as it starts, so the two
    // timestamps agree to within seconds — a near-identification. Assigned
    // globally tightest-first so the best-evidenced pair claims before a looser
    // one can take its transcript.
    //
    // Ranking by recency instead put two addev agents on each OTHER's
    // conversation: the transcript last spoken into was not the one that process
    // opened, and both cards would have drawn a neighbour's context.
    let mut pairs: Vec<(i64, u32, usize)> = Vec::new();
    for p in panes {
        if out.contains_key(&p.pid) {
            continue;
        }
        let Some(start) = p.started_at else { continue };
        for &ci in &p.cands {
            if let Some(began) = cands[ci].began {
                let d = (began - start).abs();
                if d <= BIRTH_TOLERANCE {
                    pairs.push((d, p.pid, ci));
                }
            }
        }
    }
    pairs.sort_by_key(|(d, pid, ci)| (*d, *pid, *ci));
    for (_, pid, ci) in pairs {
        if out.contains_key(&pid) || claimed.contains(&ci) {
            continue;
        }
        claimed.insert(ci);
        out.insert(pid, ci);
    }

    // Recency. A /clear mints a new conversation while the process runs on, so
    // an agent's current transcript can begin hours after it started and match
    // no birth. Among what began after the process did, the one last spoken into
    // is where it is now.
    for p in panes {
        if out.contains_key(&p.pid) {
            continue;
        }
        let free: Vec<usize> = p
            .cands
            .iter()
            .copied()
            .filter(|ci| !claimed.contains(ci) && cands[*ci].spoke.is_some())
            .collect();
        if free.is_empty() {
            continue;
        }
        let floor = p.started_at.map(|s| s - BIRTH_TOLERANCE);
        let eligible: Vec<usize> = match floor {
            Some(f) => free
                .iter()
                .copied()
                .filter(|ci| cands[*ci].began.is_none_or(|b| b >= f))
                .collect(),
            None => free.clone(),
        };
        let pool = if eligible.is_empty() {
            &free
        } else {
            &eligible
        };
        if let Some(&pick) = pool
            .iter()
            .max_by_key(|ci| cands[**ci].spoke.unwrap_or(i64::MIN))
        {
            claimed.insert(pick);
            out.insert(p.pid, pick);
        }
    }
    out
}

/// First and last record timestamps, read from the two ends of the file rather
/// than by parsing all of it. Line one is routinely a `leafUuid` record with no
/// timestamp at all, so both ends scan for the first line that carries one.
pub fn edges(path: &Path) -> Cand {
    use std::io::{Read, Seek, SeekFrom};
    const EDGE: u64 = 256 * 1024;

    let id = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut cand = Cand {
        id,
        began: None,
        spoke: None,
    };

    let Ok(mut f) = std::fs::File::open(path) else {
        return cand;
    };
    let Ok(md) = f.metadata() else { return cand };
    let len = md.len();

    let mut head = vec![0u8; EDGE.min(len) as usize];
    if f.read_exact(&mut head).is_ok() {
        let s = String::from_utf8_lossy(&head);
        // Seconds here: these are compared against /proc process start times.
        cand.began = s
            .lines()
            .find_map(|l| parse_ts(line_ts(l).as_deref()))
            .map(|ms| ms / 1000);
    }

    let from = len.saturating_sub(EDGE);
    let mut tail = vec![0u8; (len - from) as usize];
    if f.seek(SeekFrom::Start(from)).is_ok() && f.read_exact(&mut tail).is_ok() {
        let s = String::from_utf8_lossy(&tail);
        // Seeking into the middle of a line leaves a fragment that is not JSON;
        // it is skipped like any other unparseable line.
        cand.spoke = s
            .lines()
            .rev()
            .find_map(|l| parse_ts(line_ts(l).as_deref()))
            .map(|ms| ms / 1000);
    }
    cand
}

fn line_ts(line: &str) -> Option<String> {
    let line = line.trim();
    if !line.starts_with('{') {
        return None;
    }
    serde_json::from_str::<Value>(line)
        .ok()?
        .get("timestamp")?
        .as_str()
        .map(str::to_string)
}

/// The `claude` process under a pane's shell. TD reports the shell's pid; the
/// agent is a child of it, and it is the child's start time that dates the
/// conversation.
pub fn agent_under(shell_pid: u32) -> Option<u32> {
    fn kids(pid: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
            return out;
        };
        for t in tasks.flatten() {
            if let Ok(s) = std::fs::read_to_string(t.path().join("children")) {
                out.extend(s.split_whitespace().filter_map(|k| k.parse::<u32>().ok()));
            }
        }
        out
    }
    fn walk(pid: u32, depth: u8) -> Option<u32> {
        if depth > 3 {
            return None;
        }
        let children = kids(pid);
        for k in &children {
            if std::fs::read_to_string(format!("/proc/{k}/comm"))
                .map(|c| c.trim() == "claude")
                .unwrap_or(false)
            {
                return Some(*k);
            }
        }
        children.iter().find_map(|k| walk(*k, depth + 1))
    }
    walk(shell_pid, 0)
}

// ─── the fleet sweep ───────────────────────────────────────────────────────

/// What the wall knows about one agent pane before the sweep runs.
pub struct PaneReq {
    /// The pane's shell pid — the card's identity.
    pub shell_pid: u32,
    pub cwd: Option<String>,
    /// The pane's launch/resume command, if it carried one.
    pub resume: Option<String>,
    /// The stamp of whatever vitals the wall already holds for this pane.
    pub known: Option<(u64, u64)>,
}

/// What to do with a pane's card after the sweep.
pub enum Update {
    /// The transcript has not changed; keep what is on screen.
    Keep,
    /// No transcript, or nothing measurable in it yet — draw no bars.
    Clear,
    Set(Box<Vitals>),
}

/// The session id out of a `claude --resume <uuid>` command line.
///
/// Anchored on the flag, never "the first uuid in the string": every scratchpad
/// under `/tmp/claude-<uid>/` is named after a session, so a loose scan reported
/// a bash process as a live agent.
fn declared_id(resume: Option<&str>) -> Option<String> {
    let s = resume?;
    let at = s.find("--resume")?;
    let rest = s[at + "--resume".len()..].trim_start_matches(['=', ' ']);
    let id: String = rest
        .chars()
        .take_while(|c| c.is_ascii_hexdigit() || *c == '-')
        .collect();
    (id.len() == 36).then_some(id)
}

/// Measure every agent pane in one pass. Runs on the background executor: the
/// whole seventeen-agent fleet was measured at 560ms, and only transcripts that
/// have grown are re-read.
///
/// One pass rather than a per-pane lookup because the binding has to be
/// mutually exclusive — see [`assign`] and terminal-delight#272.
pub fn sweep(reqs: &[PaneReq], home: &Path) -> Vec<(u32, Update)> {
    // Candidate transcripts, per project directory, read once even when several
    // panes share a cwd.
    let mut cands: Vec<Cand> = Vec::new();
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    let mut by_dir: HashMap<String, Vec<usize>> = HashMap::new();

    for r in reqs {
        let Some(cwd) = r.cwd.as_deref() else {
            continue;
        };
        let slug = crate::session::claude_slug(cwd);
        if by_dir.contains_key(&slug) {
            continue;
        }
        let dir = home.join(".claude/projects").join(&slug);
        let mut idx = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "jsonl") {
                    idx.push(cands.len());
                    cands.push(edges(&p));
                    paths.push(p);
                }
            }
        }
        by_dir.insert(slug, idx);
    }

    let panes: Vec<FleetPane> = reqs
        .iter()
        .map(|r| {
            // The pane pid is the shell; the agent is a child, and it is the
            // CHILD's start time that dates the conversation.
            let agent = r
                .cwd
                .as_deref()
                .and_then(|_| agent_under(r.shell_pid))
                .unwrap_or(r.shell_pid);
            FleetPane {
                pid: r.shell_pid,
                declared: declared_id(r.resume.as_deref()),
                started_at: crate::session::proc_start_unix(agent).map(|s| s as i64),
                cands: r
                    .cwd
                    .as_deref()
                    .map(|c| {
                        by_dir
                            .get(&crate::session::claude_slug(c))
                            .cloned()
                            .unwrap_or_default()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect();

    let bound = assign(&panes, &cands);

    reqs.iter()
        .map(|r| {
            let Some(&ci) = bound.get(&r.shell_pid) else {
                return (r.shell_pid, Update::Clear);
            };
            let path = &paths[ci];
            if !is_stale_stamp(r.known, path) {
                return (r.shell_pid, Update::Keep);
            }
            match read(path) {
                Some(v) => (r.shell_pid, Update::Set(Box::new(v))),
                None => (r.shell_pid, Update::Clear),
            }
        })
        .collect()
}

fn is_stale_stamp(known: Option<(u64, u64)>, path: &Path) -> bool {
    let Some(k) = known else { return true };
    let Ok(md) = std::fs::metadata(path) else {
        return true;
    };
    let now = (
        md.len(),
        md.modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0),
    );
    now != k
}

// ─── the oracle CLI ────────────────────────────────────────────────────────

/// `terminal-delight agent-vitals <transcript.jsonl>` — the three bars as JSON,
/// in the same shape `scripts/td-agent-vitals.mjs --json` emits, so the two can
/// be diffed on real transcripts. `--parts` adds the fatigue components, which
/// is what a disagreement is actually localised with.
pub fn run_cli(args: &[String]) -> i32 {
    if args.is_empty() {
        eprintln!("usage: terminal-delight agent-vitals <transcript.jsonl>...");
        return 2;
    }
    let parts_wanted = args.iter().any(|a| a == "--parts");
    if args.iter().any(|a| a == "--keys") {
        for a in args.iter().filter(|a| !a.starts_with("--")) {
            let Ok(body) = std::fs::read_to_string(Path::new(a)) else {
                continue;
            };
            for c in parse_transcript(&body).calls {
                println!("{}", c.key);
            }
        }
        return 0;
    }
    let mut rows = Vec::new();
    for a in args {
        if a.starts_with("--") {
            continue;
        }
        let path = Path::new(a);
        if parts_wanted {
            if let Ok(body) = std::fs::read_to_string(path) {
                let mut t = parse_transcript(&body);
                attribute_tokens(&mut t);
                let win = window_stat(&t);
                let f = fatigue_stat(&t, win.limit);
                let r = relevance_stat(&t, &win);
                let names = ["scars", "errors", "churn", "latency", "retread", "age"];
                let mut ps: Vec<String> = names
                    .iter()
                    .zip(f.parts.iter())
                    .map(|(n, v)| format!("\"{n}\":{}", (v * 100.0).round()))
                    .collect();
                for (n, v) in [
                    "live",
                    "stale",
                    "superseded",
                    "errored",
                    "summary",
                    "resident",
                ]
                .iter()
                .zip(r.buckets.iter())
                {
                    ps.push(format!("\"{n}\":{}", v.round()));
                }
                rows.push(format!("{{{}}}", ps.join(",")));
                continue;
            }
        }
        match read(path) {
            Some(v) => rows.push(format!(
                r#"{{"session":"{}","window":{:.4},"fatigue":{:.4},"relevance":{:.4},"call":"{}","why":"{}","tokens":{},"limit":{},"turnsLeft":{},"ballast":{},"model":{},"effort":{}}}"#,
                path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                v.window.fill,
                v.fatigue.fill,
                v.relevance.fill,
                v.call.label(),
                v.why.replace('"', "'"),
                v.tokens,
                v.limit,
                v.turns_left.map(|n| n.to_string()).unwrap_or_else(|| "null".into()),
                v.ballast,
                v.model.as_deref().map(|m| format!("\"{m}\"")).unwrap_or_else(|| "null".into()),
                v.effort.as_deref().map(|e| format!("\"{e}\"")).unwrap_or_else(|| "null".into()),
            )),
            None => eprintln!("{a}: no assistant turns"),
        }
    }
    println!("[{}]", rows.join(","));
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── fixtures ───────────────────────────────────────────────────────────

    struct Build {
        rows: Vec<String>,
        clock: i64,
    }

    impl Build {
        fn new() -> Self {
            Build {
                rows: vec![],
                clock: 1_788_000_000,
            }
        }
        fn ts(&mut self) -> String {
            self.clock += 30;
            // 2026-09-02T…Z — only the ordering and spacing matter to the maths.
            let d = self.clock;
            let (h, m, s) = ((d / 3600) % 24, (d / 60) % 60, d % 60);
            format!("2026-09-02T{h:02}:{m:02}:{s:02}.000Z")
        }
        fn assistant(
            &mut self,
            ctx: u64,
            created: u64,
            out: u64,
            calls: &[(&str, &str, &str)],
        ) -> &mut Self {
            let ts = self.ts();
            let content: Vec<String> = calls
                .iter()
                .map(|(id, name, path)| {
                    format!(
                        r#"{{"type":"tool_use","id":"{id}","name":"{name}","input":{{"file_path":"{path}"}}}}"#
                    )
                })
                .collect();
            self.rows.push(format!(
                r#"{{"type":"assistant","timestamp":"{ts}","effort":"max","message":{{"model":"claude-opus-5","usage":{{"input_tokens":2,"cache_creation_input_tokens":{created},"cache_read_input_tokens":{},"output_tokens":{out}}},"content":[{}]}}}}"#,
                ctx.saturating_sub(created + 2),
                content.join(",")
            ));
            self
        }
        fn results(&mut self, items: &[(&str, usize, bool)]) -> &mut Self {
            let ts = self.ts();
            let content: Vec<String> = items
                .iter()
                .map(|(id, chars, err)| {
                    format!(
                        r#"{{"type":"tool_result","tool_use_id":"{id}","is_error":{err},"content":"{}"}}"#,
                        "x".repeat(*chars)
                    )
                })
                .collect();
            self.rows.push(format!(
                r#"{{"type":"user","timestamp":"{ts}","message":{{"content":[{}]}}}}"#,
                content.join(",")
            ));
            self
        }
        fn prompt(&mut self) -> &mut Self {
            let ts = self.ts();
            self.rows.push(format!(
                r#"{{"type":"user","timestamp":"{ts}","message":{{"content":"do the thing"}}}}"#
            ));
            self
        }
        fn compaction(&mut self, post: u64, dropped: u64) -> &mut Self {
            let ts = self.ts();
            self.rows.push(format!(
                r#"{{"type":"system","subtype":"compact_boundary","timestamp":"{ts}","compactMetadata":{{"trigger":"manual","postTokens":{post},"cumulativeDroppedTokens":{dropped}}}}}"#
            ));
            self
        }
        fn body(&self) -> String {
            self.rows.join("\n")
        }
    }

    /// `n` read/result pairs cycling `files`, each result `chars` long.
    fn session(n: usize, files: &[&str], chars: usize) -> String {
        let mut b = Build::new();
        b.prompt();
        let mut ctx = 20_000u64;
        for i in 0..n {
            let id = format!("c{i}");
            ctx += (chars / 4) as u64;
            b.assistant(
                ctx,
                (chars / 4) as u64,
                100,
                &[(&id, "Read", files[i % files.len()])],
            );
            b.results(&[(&id, chars, false)]);
        }
        b.assistant(ctx, 0, 50, &[]);
        b.body()
    }

    fn v(body: &str) -> Vitals {
        from_body(body, (0, 0)).expect("a transcript with turns yields vitals")
    }

    // ── the bar's own maths ────────────────────────────────────────────────

    #[test]
    fn a_bar_always_draws_even_at_zero() {
        let b = Bar {
            label: "X",
            fill: 0.0,
            tone: Tone::Bad,
            charge: 0.0,
            caption: String::new(),
        };
        assert_eq!(b.drawn(), MIN_FILL);
    }

    #[test]
    fn window_stays_white_through_the_comfortable_range() {
        // The rule Parker set: white while rising is neither good nor bad. A
        // half-full context really is fine, and a bar that reddens there is
        // crying wolf on twelve cards at once.
        assert_eq!(smoothstep(WINDOW_CALM, WINDOW_ALARM, 0.10), 0.0);
        assert_eq!(smoothstep(WINDOW_CALM, WINDOW_ALARM, 0.55), 0.0);
        assert!(smoothstep(WINDOW_CALM, WINDOW_ALARM, 0.75) > 0.1);
        assert!(smoothstep(WINDOW_CALM, WINDOW_ALARM, 0.95) > 0.9);
        assert_eq!(smoothstep(WINDOW_CALM, WINDOW_ALARM, 1.0), 1.0);
    }

    #[test]
    fn charge_is_monotonic_so_worse_never_looks_calmer() {
        let mut prev = -1.0;
        for i in 0..=100 {
            let c = smoothstep(WINDOW_CALM, WINDOW_ALARM, i as f32 / 100.0);
            assert!(c >= prev, "charge dipped at {i}%");
            prev = c;
        }
    }

    // ── context limit ──────────────────────────────────────────────────────

    #[test]
    fn the_1m_variant_is_read_from_cost_state() {
        // `.message.model` says plain `claude-opus-5` even in a 1M session.
        // Trusting it reported a 550k window as 275% full.
        let models = vec!["claude-opus-5[1m]".to_string()];
        assert_eq!(
            context_limit(&models, Some("claude-opus-5"), 549_905),
            1_000_000
        );
    }

    #[test]
    fn a_window_that_held_more_than_nominal_is_not_nominal() {
        assert_eq!(
            context_limit(&[], Some("claude-opus-5"), 549_905),
            1_000_000
        );
    }

    #[test]
    fn an_ordinary_session_keeps_the_nominal_limit() {
        assert_eq!(context_limit(&[], Some("claude-opus-5"), 120_000), 200_000);
    }

    // ── the provider chip ──────────────────────────────────────────────────

    #[test]
    fn the_chip_is_family_and_effort_never_a_version() {
        assert_eq!(model_name("claude-opus-5").as_deref(), Some("OPUS"));
        assert_eq!(model_name("claude-opus-4-8").as_deref(), Some("OPUS"));
        assert_eq!(model_name("claude-fable-5").as_deref(), Some("FABLE"));
        assert_eq!(model_name("gpt-5.6-sol").as_deref(), Some("GPT"));
        assert_eq!(model_name("<synthetic>"), None);
    }

    #[test]
    fn the_chip_reads_opus_max() {
        let vit = v(&session(6, &["a.rs", "b.rs"], 4000));
        assert_eq!(vit.chip().as_deref(), Some("OPUS \u{b7} MAX"));
    }

    #[test]
    fn effort_comes_from_the_record_because_the_status_line_cannot_say_max() {
        // hud::extract_effort knows only high/medium/low, so scraping the status
        // line could never have produced MAX — the value on 7,600 real turns.
        let vit = v(&session(4, &["a.rs"], 4000));
        assert_eq!(vit.effort.as_deref(), Some("MAX"));
    }

    // ── window ─────────────────────────────────────────────────────────────

    #[test]
    fn window_is_the_sum_of_the_last_turns_three_input_figures() {
        let mut b = Build::new();
        b.assistant(40_000, 1_000, 100, &[])
            .assistant(60_000, 2_000, 100, &[]);
        let vit = v(&b.body());
        assert_eq!(vit.tokens, 60_000);
        assert_eq!(vit.limit, 200_000);
        assert_eq!(vit.window.caption, "30%");
    }

    #[test]
    fn headroom_is_counted_in_turns_at_the_recent_rate() {
        let mut b = Build::new();
        let mut ctx = 10_000;
        for _ in 0..14 {
            ctx += 1_000;
            b.assistant(ctx, 1_000, 100, &[]);
        }
        let vit = v(&b.body());
        // 92% of 200k is 184k; from 24k at 1k a turn.
        assert_eq!(vit.turns_left, Some(160));
    }

    // ── token attribution ──────────────────────────────────────────────────

    #[test]
    fn parallel_calls_split_one_cache_write_rather_than_each_taking_it_all() {
        let mut b = Build::new();
        b.assistant(
            10_000,
            0,
            100,
            &[("a", "Read", "a.rs"), ("b", "Read", "b.rs")],
        )
        .results(&[("a", 4_000, false), ("b", 4_000, false)])
        .assistant(12_100, 2_100, 50, &[]);
        let mut t = parse_transcript(&b.body());
        attribute_tokens(&mut t);
        // 2,100 written minus 100 output = 2,000, split evenly by size.
        assert_eq!(t.results[0].tokens, 1_000);
        assert_eq!(t.results[1].tokens, 1_000);
    }

    #[test]
    fn a_cache_figure_that_dwarfs_its_text_is_not_believed() {
        // Eleven errored results totalling 3,800 characters were charged 197,400
        // tokens, because cache_creation also covers pasted messages and hook
        // output. A 300-character error is not 18k tokens.
        let mut b = Build::new();
        b.assistant(10_000, 0, 100, &[("a", "Bash", "x.sh")])
            .results(&[("a", 300, true)])
            .assistant(60_100, 50_100, 100, &[]);
        let mut t = parse_transcript(&b.body());
        attribute_tokens(&mut t);
        assert_eq!(t.results[0].tokens, 75); // 300/4, not 50,000
    }

    // ── call targets ───────────────────────────────────────────────────────

    #[test]
    fn a_path_is_lifted_out_of_a_shell_command() {
        let input = serde_json::json!({"command": "grep -n foo app/src/main.rs"});
        let (paths, _) = targets_of("Bash", &input);
        assert!(paths.iter().any(|p| p == "app/src/main.rs"));
    }

    #[test]
    fn a_dotted_module_name_is_not_a_path() {
        // `python3 -m http.server` contributed `http.server` to the focus set,
        // and one junk entry there scores unrelated work as on-topic.
        let input = serde_json::json!({"command": "python3 -m http.server 8000"});
        let (paths, _) = targets_of("Bash", &input);
        assert!(paths.is_empty(), "got {paths:?}");
    }

    // Each of the four below was a real disagreement with the reference
    // implementation, found by diffing call keys over live transcripts. None
    // would have failed a unit test written from the spec: both sides were
    // self-consistent and only one was right.

    #[test]
    fn a_trailing_slash_is_not_part_of_the_name() {
        // `app/src/` and `app/src` are one directory. Keeping the slash made
        // them two, splitting a focus entry and moving churn by two points.
        let input = serde_json::json!({"command": "ls app/src/ and app/src"});
        let (paths, _) = targets_of("Bash", &input);
        assert_eq!(paths, vec!["app/src".to_string()]);
    }

    #[test]
    fn a_path_cannot_cross_an_empty_segment() {
        // `jq '.conclusion//.state'` is not a file. Swallowing the run whole
        // gave it a slash, which made it look like one.
        let input = serde_json::json!({"command": "jq '.conclusion//.state' out.json"});
        let (paths, _) = targets_of("Bash", &input);
        assert_eq!(paths, vec!["out.json".to_string()]);
    }

    #[test]
    fn a_run_must_follow_a_delimiter_to_count() {
        // In `foo:bar/baz.rs` the reference sees `foo` (rejected: no extension)
        // and never `bar/baz.rs`, because nothing delimits it.
        let input = serde_json::json!({"command": "echo foo:bar/baz.rs"});
        let (paths, _) = targets_of("Bash", &input);
        assert!(paths.is_empty(), "got {paths:?}");
    }

    #[test]
    fn a_tilde_path_is_the_same_file_as_its_expansion() {
        // Leaving `~/` unexpanded split one file into two identities, and 4.4k
        // tokens that were superseded copies scored as live context.
        let home = std::env::var("HOME").unwrap_or_default();
        let input = serde_json::json!({"command": "cat ~/Work/x.rs"});
        let (paths, _) = targets_of("Bash", &input);
        assert_eq!(paths, vec![format!("{home}/Work/x.rs")]);
    }

    #[test]
    fn a_bare_slash_names_nothing() {
        let input = serde_json::json!({"command": "ls / && df /"});
        let (paths, _) = targets_of("Bash", &input);
        assert!(paths.is_empty(), "got {paths:?}");
    }

    #[test]
    fn the_call_key_reads_one_field_not_all_of_them() {
        // A Bash call carries `command` AND `description`; joining both made the
        // key change whenever the description did, splitting repeats of one
        // command into separate keys and moving churn.
        let a = targets_of(
            "Bash",
            &serde_json::json!({"command": "git status", "description": "check"}),
        );
        let b = targets_of(
            "Bash",
            &serde_json::json!({"command": "git status", "description": "look again"}),
        );
        assert_eq!(a.1, b.1, "the same command is the same call");
    }

    #[test]
    fn two_different_shell_commands_are_two_different_calls() {
        // Keying a path-less call by tool name alone made every shell call
        // supersede every other one, scoring a healthy transcript at 8%.
        let (_, a) = targets_of("Bash", &serde_json::json!({"command": "git status"}));
        let (_, b) = targets_of("Bash", &serde_json::json!({"command": "cargo test"}));
        assert_ne!(a, b);
    }

    // ── relevance ──────────────────────────────────────────────────────────

    #[test]
    fn a_session_reading_a_set_of_files_once_each_scores_high() {
        let files: Vec<String> = (0..20).map(|i| format!("app/src/f{i}.rs")).collect();
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        assert!(v(&session(20, &refs, 4000)).relevance.fill > 0.70);
    }

    #[test]
    fn the_same_file_fetched_twenty_times_is_nineteen_dead_copies() {
        // Not a defect in the measure — the window really is carrying nineteen
        // superseded copies, and that is what low relevance means.
        assert!(v(&session(20, &["app/src/main.rs"], 4000)).relevance.fill < 0.40);
    }

    #[test]
    fn relevance_is_measured_over_the_window_not_everything_ever_loaded() {
        // Summing every result a long session fetched put 1.6M tokens of
        // "context" inside a 456k window.
        let vit = v(&session(400, &["a.rs", "b.rs", "c.rs"], 8000));
        assert!(
            vit.ballast <= vit.tokens,
            "ballast {} exceeds window {}",
            vit.ballast,
            vit.tokens
        );
    }

    #[test]
    fn content_dropped_at_a_compaction_leaves_the_denominator_with_it() {
        let mut b = Build::new();
        b.prompt();
        let mut ctx = 20_000u64;
        for i in 0..30 {
            let id = format!("o{i}");
            ctx += 1_000;
            b.assistant(ctx, 1_000, 100, &[(&id, "Read", "old.rs")])
                .results(&[(&id, 4_000, false)]);
        }
        b.compaction(12_000, 168_000);
        ctx = 12_000;
        for i in 0..10 {
            let id = format!("n{i}");
            ctx += 1_000;
            b.assistant(ctx, 1_000, 100, &[(&id, "Read", "new.rs")])
                .results(&[(&id, 4_000, false)]);
        }
        b.assistant(ctx, 0, 50, &[]);
        let vit = v(&b.body());
        assert!(vit.ballast <= vit.tokens);
    }

    // ── fatigue ────────────────────────────────────────────────────────────

    #[test]
    fn a_clean_short_session_is_not_tired() {
        let vit = v(&session(12, &["a.rs", "b.rs", "c.rs", "d.rs"], 4000));
        assert!(vit.fatigue.fill < 0.35, "scored {}", vit.fatigue.fill);
    }

    #[test]
    fn surviving_a_compaction_is_a_scar() {
        let clean = v(&session(20, &["a.rs"], 4000));
        let mut b = Build::new();
        b.prompt().compaction(20_000, 160_000);
        let mut ctx = 20_000u64;
        for i in 0..20 {
            let id = format!("c{i}");
            ctx += 1_000;
            b.assistant(ctx, 1_000, 100, &[(&id, "Read", "a.rs")])
                .results(&[(&id, 4_000, false)]);
        }
        b.assistant(ctx, 0, 50, &[]);
        let scarred = v(&b.body());
        assert!(scarred.fatigue.fill > clean.fatigue.fill);
    }

    #[test]
    fn iterating_on_two_files_twice_over_is_not_churn() {
        // The third time, not the second. Counting a repeat pinned every working
        // session at 100 and measured nothing.
        let vit = v(&session(4, &["a.rs", "b.rs"], 4000));
        assert!(vit.fatigue.fill < 0.2);
    }

    #[test]
    fn two_late_errors_are_not_a_trend() {
        // A session with 2 errors in 81 calls read 100 for "errors climbing",
        // because both landed recently and the ratio against zero is unbounded.
        let mut b = Build::new();
        b.prompt();
        let mut ctx = 20_000u64;
        for i in 0..40 {
            let id = format!("c{i}");
            ctx += 1_000;
            b.assistant(ctx, 1_000, 100, &[(&id, "Read", "a.rs")])
                .results(&[(&id, 4_000, i >= 38)]);
        }
        b.assistant(ctx, 0, 50, &[]);
        let t = parse_transcript(&b.body());
        let f = fatigue_stat(&t, 200_000);
        assert!(!f.reason.contains("climbing"), "reason was {}", f.reason);
    }

    // ── the call ───────────────────────────────────────────────────────────

    fn at(fill: f64, rel: f64, fatigue: f64, limit: u64) -> (Call, String) {
        let win = WindowStat {
            tokens: (fill * limit as f64) as u64,
            limit,
            fill,
            growth: 500,
            turns_left: Some(200),
        };
        let fat = FatigueStat {
            score: fatigue,
            reason: "quiet".into(),
            parts: [0.; 6],
        };
        let r = RelevanceStat {
            score: rel,
            ballast: ((1.0 - rel) * win.tokens as f64) as u64,
            buckets: [0.; 6],
        };
        verdict(&win, &fat, &r)
    }

    #[test]
    fn a_full_window_still_in_use_is_a_hand_off_not_a_compaction() {
        // The whole point of the pair: identical on a token counter, opposite
        // treatment. Compacting here destroys detail in use.
        assert_eq!(at(0.91, 0.93, 0.10, 200_000).0, Call::HandOff);
    }

    #[test]
    fn a_full_window_of_ballast_is_a_cheap_compaction() {
        assert_eq!(at(0.90, 0.30, 0.10, 200_000).0, Call::Compact);
    }

    #[test]
    fn a_fresh_focused_session_runs() {
        assert_eq!(at(0.20, 0.90, 0.10, 1_000_000).0, Call::Run);
    }

    #[test]
    fn fatigue_overrides_a_comfortable_window() {
        assert_eq!(at(0.30, 0.90, 0.75, 1_000_000).0, Call::Stop);
    }

    #[test]
    fn ballast_is_judged_against_the_window_it_sits_in() {
        // A flat threshold judged a 1M context by a 200k yardstick and fired on
        // nearly every long session. A call that is always the same call is not
        // a call.
        assert_eq!(at(0.70, 0.35, 0.10, 200_000).0, Call::Compact);
        assert_eq!(at(0.20, 0.88, 0.10, 1_000_000).0, Call::Run);
    }

    #[test]
    fn needs_you_is_the_pair_a_human_must_decide() {
        assert!(Call::Stop.needs_you() && Call::HandOff.needs_you());
        assert!(!Call::Compact.needs_you() && !Call::Run.needs_you());
    }

    // ── timestamps ─────────────────────────────────────────────────────────

    #[test]
    fn timestamps_parse_without_a_date_crate() {
        // Milliseconds, not seconds — sub-second turn gaps are real and drop out
        // of the latency sample at second resolution.
        assert_eq!(
            parse_ts(Some("2026-09-02T18:45:11.840Z")),
            Some(1_788_374_711_840)
        );
        assert_eq!(
            parse_ts(Some("2026-09-02T18:45:11Z")),
            Some(1_788_374_711_000)
        );
        assert_eq!(parse_ts(Some("1970-01-01T00:00:00.000Z")), Some(0));
        assert_eq!(parse_ts(Some("not a date")), None);
        assert_eq!(parse_ts(None), None);
    }

    #[test]
    fn an_empty_transcript_yields_no_vitals_rather_than_empty_bars() {
        assert!(from_body("", (0, 0)).is_none());
        assert!(from_body("{\"type\":\"user\"}", (0, 0)).is_none());
    }

    // ── binding a pane to its own transcript ───────────────────────────────
    //
    // Both collisions below are real, captured off this machine on 2026-09-02
    // with seventeen agents running. See terminal-delight#272.

    /// `2026-09-02T20:15:46Z` → unix SECONDS, for readable fixtures.
    ///
    /// Seconds, not the milliseconds `parse_ts` returns: the fleet assignment
    /// compares transcript edges against `/proc` process start times, and those
    /// are seconds. [`edges`] does the same divide.
    fn t(hms: &str) -> i64 {
        parse_ts(Some(&format!("2026-09-02T{hms}.000Z"))).expect("fixture parses") / 1000
    }

    fn cand(id: &str, began: &str, spoke: &str) -> Cand {
        Cand {
            id: id.into(),
            began: Some(t(began)),
            spoke: Some(t(spoke)),
        }
    }

    #[test]
    fn an_agent_gets_the_conversation_that_began_when_it_started() {
        // The three addev agents. Ranking by "last spoken into" put two of them
        // on each other's transcript: e6bcd0ef was the most recently active, so
        // whichever pane was considered first took it — but it was opened by the
        // OTHER process, two seconds after that one started.
        let cands = vec![
            cand("e6bcd0ef", "20:15:46", "23:47:42"),
            cand("f88ffe3f", "18:37:24", "18:39:37"),
            cand("104e5de3", "20:15:46", "21:49:09"),
        ];
        let all = vec![0, 1, 2];
        let panes = vec![
            FleetPane {
                pid: 3862538,
                declared: None,
                started_at: Some(t("18:37:21")),
                cands: all.clone(),
            },
            FleetPane {
                pid: 784585,
                declared: None,
                started_at: Some(t("20:15:44")),
                cands: all.clone(),
            },
            FleetPane {
                pid: 1194661,
                declared: Some("104e5de3".into()),
                started_at: Some(t("20:42:58")),
                cands: all,
            },
        ];
        let got = assign(&panes, &cands);
        assert_eq!(cands[got[&3862538]].id, "f88ffe3f");
        assert_eq!(cands[got[&784585]].id, "e6bcd0ef");
        assert_eq!(cands[got[&1194661]].id, "104e5de3");
    }

    #[test]
    fn after_a_clear_the_agent_is_in_the_newer_conversation() {
        // The Akshat pane: process up since 20:26Z, but the conversation it is
        // in began at 22:26Z. Its predecessor stopped being spoken into at 22:23
        // while its FILE kept being touched — which is why mtime is not the
        // signal and the last record is.
        let cands = vec![
            cand("5f07a6ad", "21:15:13", "22:23:52"),
            cand("6697c339", "22:26:06", "22:41:22"),
        ];
        let panes = vec![FleetPane {
            pid: 2879008,
            declared: None,
            started_at: Some(t("20:26:04")),
            cands: vec![0, 1],
        }];
        assert_eq!(cands[assign(&panes, &cands)[&2879008]].id, "6697c339");
    }

    #[test]
    fn two_panes_never_share_one_conversation() {
        // The defect itself: `claude_transcript` falls through to newest-by-mtime,
        // so two bare `claude` panes in one directory both got the same file.
        let cands = vec![
            cand("aaaaaaaa", "10:00:00", "12:00:00"),
            cand("bbbbbbbb", "11:00:00", "13:00:00"),
        ];
        let panes = vec![
            FleetPane {
                pid: 111,
                declared: None,
                started_at: Some(t("10:00:00")),
                cands: vec![0, 1],
            },
            FleetPane {
                pid: 222,
                declared: None,
                started_at: Some(t("11:00:00")),
                cands: vec![0, 1],
            },
        ];
        let got = assign(&panes, &cands);
        assert_eq!(got.len(), 2);
        assert_ne!(
            got[&111], got[&222],
            "two agents cannot hold one conversation"
        );
    }

    #[test]
    fn a_declared_resume_outranks_any_forensic_guess() {
        let cands = vec![
            cand("aaaaaaaa", "10:00:00", "23:00:00"),
            cand("bbbbbbbb", "11:00:00", "12:00:00"),
        ];
        // Birth and recency both point at aaaaaaaa; the agent says otherwise.
        let panes = vec![FleetPane {
            pid: 111,
            declared: Some("bbbbbbbb".into()),
            started_at: Some(t("10:00:00")),
            cands: vec![0, 1],
        }];
        assert_eq!(cands[assign(&panes, &cands)[&111]].id, "bbbbbbbb");
    }

    #[test]
    fn a_pane_with_no_candidate_is_left_unbound_rather_than_given_someone_elses() {
        let cands = vec![cand("aaaaaaaa", "10:00:00", "12:00:00")];
        let panes = vec![
            FleetPane {
                pid: 111,
                declared: None,
                started_at: Some(t("10:00:00")),
                cands: vec![0],
            },
            FleetPane {
                pid: 222,
                declared: None,
                started_at: Some(t("11:00:00")),
                cands: vec![0],
            },
        ];
        let got = assign(&panes, &cands);
        assert_eq!(
            got.len(),
            1,
            "the second pane draws no bars rather than the first's"
        );
    }

    #[test]
    fn assignment_does_not_depend_on_pane_order() {
        // Greedy assignment is only trustworthy if it is deterministic: the wall
        // iterates tabs in whatever order they sit in.
        let cands = vec![
            cand("e6bcd0ef", "20:15:46", "23:47:42"),
            cand("f88ffe3f", "18:37:24", "18:39:37"),
        ];
        let a = FleetPane {
            pid: 1,
            declared: None,
            started_at: Some(t("18:37:21")),
            cands: vec![0, 1],
        };
        let b = FleetPane {
            pid: 2,
            declared: None,
            started_at: Some(t("20:15:44")),
            cands: vec![0, 1],
        };
        let fwd = assign(&[a.clone(), b.clone()], &cands);
        let rev = assign(&[b, a], &cands);
        assert_eq!(fwd, rev);
        assert_eq!(cands[fwd[&1]].id, "f88ffe3f");
    }
}
