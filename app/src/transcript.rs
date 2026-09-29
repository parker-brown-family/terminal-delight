//! An agent's conversation, read from its own transcript, for the FOCUS reader.
//!
//! # Why the transcript
//!
//! Claude Code draws its replies for the pane it is in: it breaks every line at
//! the pane's width and starts each continuation two columns in. So the screen
//! holds a reply only as wide as the pane, and nothing downstream can widen it
//! again — the reader's healer refuses a row that starts indented, and is right
//! to (measured on 2026-09-29: five rows at 48 columns stayed five rows in a
//! 150-column reader). The transcript holds each reply whole, before any pane
//! broke it. Read from there, a reply fills whatever width the reader has.
//!
//! # What is here
//!
//! - [`Tail`] reads a transcript as it grows: the bytes appended since the last
//!   read, whole lines only, and a fresh start when the file shrank or was
//!   replaced.
//! - [`Conversation`] folds an agent's records into what a person reads: the
//!   prompts, the replies (a message's text blocks joined), one entry per tool
//!   call with its result attached, and a divider where the conversation was
//!   compacted. Thinking, attachments, modes, file history, queue records and
//!   subagents' sidechains are left out — the census approved at the reader's
//!   second gate (`docs/plans/alt-r-reader/`). It reads Claude Code's
//!   transcripts and Codex's rollouts alike: the two formats' records never
//!   share a `type`, so one fold takes either, line by line.
//! - [`draw`] lays a conversation out as the reader's [`Document`], in the roles
//!   the reader's first brief drew: YOU and the agent's labels, prose, headings,
//!   tables as aligned text, and one line per tool call, whose output opens on a
//!   click.
//!
//! None of it touches gpui state or the main thread. A transcript can be tens of
//! megabytes, so the reader reads and folds it in the background and hands the
//! drawn document across.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use gpui::{Font, FontStyle, FontWeight, Hsla, TextRun};
use serde_json::Value;

use crate::doc::{DocLine, Document};
use crate::docview::markdown::{self, Block, Inline};

// ── reading the file ────────────────────────────────────────────────────────

/// A transcript read as it grows.
pub struct Tail {
    path: PathBuf,
    /// Bytes taken from the file so far, the held-back partial line included.
    read: u64,
    /// The file's inode at the last read: a transcript replaced by another at
    /// the same path is another conversation, even when it is longer.
    ino: Option<u64>,
    /// A line still being written, held until its newline arrives.
    partial: Vec<u8>,
}

/// What one [`Tail::poll`] found.
#[derive(Debug, Default, PartialEq)]
pub struct Polled {
    /// The file shrank or was replaced: everything read before it is void.
    pub reset: bool,
    /// Whole lines appended since the last poll, oldest first.
    pub lines: Vec<String>,
}

impl Tail {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            read: 0,
            ino: None,
            partial: Vec::new(),
        }
    }

    /// Read what was appended since the last poll. Only whole lines come back:
    /// Claude Code writes a record as one line, and one caught mid-write is held
    /// until its newline lands rather than parsed in half.
    pub fn poll(&mut self) -> std::io::Result<Polled> {
        let mut f = std::fs::File::open(&self.path)?;
        let meta = f.metadata()?;
        let len = meta.len();
        #[cfg(unix)]
        let ino = Some(std::os::unix::fs::MetadataExt::ino(&meta));
        #[cfg(not(unix))]
        let ino = None;
        let mut out = Polled::default();
        if len < self.read || (self.ino.is_some() && self.ino != ino) {
            self.read = 0;
            self.partial.clear();
            out.reset = true;
        }
        self.ino = ino;
        if len > self.read {
            f.seek(SeekFrom::Start(self.read))?;
            let mut buf = Vec::with_capacity((len - self.read) as usize);
            (&mut f).take(len - self.read).read_to_end(&mut buf)?;
            self.read += buf.len() as u64;
            self.partial.extend_from_slice(&buf);
        }
        if let Some(end) = self.partial.iter().rposition(|&b| b == b'\n') {
            // The unfinished line moves out, and the whole ones stay where they
            // were read: a first read can be tens of megabytes.
            let unfinished = self.partial.split_off(end + 1);
            let whole = std::mem::replace(&mut self.partial, unfinished);
            out.lines = whole
                .split(|&b| b == b'\n')
                .filter(|l| !l.is_empty())
                .map(|l| String::from_utf8_lossy(l).into_owned())
                .collect();
        }
        Ok(out)
    }
}

// ── the conversation ────────────────────────────────────────────────────────

/// How much of a tool's output is kept for opening: enough to read, and a
/// bounded cost for a conversation that ran a thousand of them.
const KEEP_LINES: usize = 400;
const KEEP_BYTES: usize = 64 * 1024;

/// One thing a person reads in a conversation.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// What was typed to the agent.
    Prompt(String),
    /// What the agent said, as Markdown: the text blocks of one message, joined.
    Reply(String),
    /// A tool the agent called, and what came back once it has.
    Tool(ToolCall),
    /// The conversation was compacted here; what came before is a summary now.
    Compacted,
}

/// A tool call, as one line of the conversation.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    /// The tool, with an MCP server's namespace taken off: `ctx_read`.
    pub name: String,
    /// What it was pointed at, whole: the command as it was written, the file,
    /// the pattern. Its row draws as much as fits on one line.
    pub target: String,
    /// What came back. `None` while the call is still running.
    pub result: Option<ToolOut>,
}

/// What a tool call printed.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOut {
    /// One short line for the call's own row: "523 lines", the first line.
    pub summary: String,
    /// What it printed, up to [`KEEP_LINES`] lines or [`KEEP_BYTES`] bytes.
    pub text: String,
    /// Lines left out of `text`.
    pub cut: usize,
    pub error: bool,
}

/// A conversation, folded from its transcript one record at a time.
#[derive(Default)]
pub struct Conversation {
    entries: Vec<Entry>,
    /// Moves on every change, so a drawing can be kept until it is stale.
    rev: u64,
    /// The message the last entry replies from, while more of its text may
    /// follow: Claude Code writes each block of a message as its own record.
    open_reply: Option<String>,
    /// Where each tool call is, by its id, for its result to find it.
    tools: HashMap<String, usize>,
    /// The directory a Codex session works in, from its opening record: Codex
    /// states it once, where Claude Code stamps it on every record.
    cwd: Option<String>,
}

impl Conversation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn rev(&self) -> u64 {
        self.rev
    }

    /// Fold one transcript line in. A line that is not a record, or a record a
    /// person does not read, changes nothing.
    pub fn fold(&mut self, line: &str) {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return;
        };
        // A subagent's own conversation is its own, and has its own file.
        if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            return;
        }
        match v.get("type").and_then(Value::as_str) {
            Some("user") => self.user(&v),
            Some("assistant") => self.assistant(&v),
            Some("system")
                if v.get("subtype").and_then(Value::as_str) == Some("compact_boundary") =>
            {
                self.push(Entry::Compacted)
            }
            Some("attachment") => self.queued(&v),
            // Codex's rollouts: every record carries its body as a payload.
            Some("response_item") => {
                if let Some(p) = v.get("payload") {
                    self.codex(p);
                }
            }
            Some("compacted") => self.push(Entry::Compacted),
            Some("session_meta" | "turn_context") => {
                if let Some(cwd) = v.pointer("/payload/cwd").and_then(Value::as_str) {
                    self.cwd = Some(cwd.to_string());
                }
            }
            _ => {}
        }
    }

    fn push(&mut self, entry: Entry) {
        self.entries.push(entry);
        self.open_reply = None;
        self.rev += 1;
    }

    /// A user record: a prompt, or the results of tool calls. Records Claude
    /// Code wrote itself (`isMeta`) are not what anybody typed, and the long
    /// summary after a compaction is not either — the divider stands for it.
    fn user(&mut self, v: &Value) {
        let meta = v.get("isMeta").and_then(Value::as_bool) == Some(true);
        if v.get("isCompactSummary").and_then(Value::as_bool) == Some(true) {
            return;
        }
        let Some(content) = v.pointer("/message/content") else {
            return;
        };
        if let Value::Array(blocks) = content {
            for b in blocks {
                if b.get("type").and_then(Value::as_str) == Some("tool_result") {
                    self.result(b);
                }
            }
        }
        if let Some(text) = typed(content).filter(|_| !meta) {
            self.prompt(&text);
        }
    }

    fn prompt(&mut self, text: &str) {
        if !text.trim().is_empty() {
            self.push(Entry::Prompt(text.trim_end().to_string()));
        }
    }

    /// Something typed while the agent was busy, queued for its next turn. The
    /// queue also carries what nobody typed — a background task finishing, a
    /// message from another agent — which Claude Code files under another mode
    /// or marks as meta.
    fn queued(&mut self, v: &Value) {
        let Some(a) = v.get("attachment") else {
            return;
        };
        if a.get("type").and_then(Value::as_str) != Some("queued_command")
            || a.get("commandMode").and_then(Value::as_str) != Some("prompt")
            || a.get("isMeta").and_then(Value::as_bool) == Some(true)
        {
            return;
        }
        if let Some(text) = a.get("prompt").and_then(typed) {
            self.prompt(&text);
        }
    }

    /// An assistant record: text, a tool call, or thinking, which is not shown.
    fn assistant(&mut self, v: &Value) {
        let id = v
            .pointer("/message/id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let Some(blocks) = v.pointer("/message/content").and_then(Value::as_array) else {
            return;
        };
        for b in blocks {
            match b.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(t) = b.get("text").and_then(Value::as_str) {
                        self.reply(&id, t);
                    }
                }
                Some("tool_use") => self.call(
                    b.get("name").and_then(Value::as_str),
                    b.get("input").unwrap_or(&Value::Null),
                    b.get("id").and_then(Value::as_str),
                    v.get("cwd").and_then(Value::as_str),
                ),
                _ => {}
            }
        }
    }

    /// A Codex response item: a message, a tool call, or what a call printed.
    /// Reasoning, and the developer's instructions, are not shown.
    fn codex(&mut self, p: &Value) {
        let id = p.get("call_id").and_then(Value::as_str);
        match p.get("type").and_then(Value::as_str) {
            Some("message") => match p.get("role").and_then(Value::as_str) {
                Some("user") => {
                    if let Some(text) = codex_typed(p) {
                        self.prompt(&text);
                    }
                }
                Some("assistant") => {
                    let (text, _) = result_text(p.get("content"));
                    let message = p.get("id").and_then(Value::as_str).unwrap_or_default();
                    self.reply(message, &text);
                }
                _ => {}
            },
            // `exec` hands over a script as its input; a function, its
            // arguments as a JSON string.
            Some("custom_tool_call") => {
                let input = p.get("input").unwrap_or(&Value::Null).clone();
                let cwd = self.cwd.clone();
                self.call(
                    p.get("name").and_then(Value::as_str),
                    &input,
                    id,
                    cwd.as_deref(),
                );
            }
            Some("function_call") => {
                let input = p
                    .get("arguments")
                    .and_then(Value::as_str)
                    .and_then(|a| serde_json::from_str::<Value>(a).ok())
                    .unwrap_or(Value::Null);
                let cwd = self.cwd.clone();
                self.call(
                    p.get("name").and_then(Value::as_str),
                    &input,
                    id,
                    cwd.as_deref(),
                );
            }
            Some("custom_tool_call_output" | "function_call_output") => {
                let (text, pictures) = result_text(p.get("output"));
                let (said, failed) = unwrapped(&text);
                self.answer(id, said, pictures, failed);
            }
            _ => {}
        }
    }

    /// Text from message `id`: joined onto the reply just before it when that
    /// is the same message's, so a reply reads as one.
    fn reply(&mut self, id: &str, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        if let (Some(open), Some(Entry::Reply(so_far))) =
            (self.open_reply.as_deref(), self.entries.last_mut())
        {
            if !id.is_empty() && open == id {
                so_far.push_str("\n\n");
                so_far.push_str(text.trim_end());
                self.rev += 1;
                return;
            }
        }
        self.push(Entry::Reply(text.trim_end().to_string()));
        self.open_reply = (!id.is_empty()).then(|| id.to_string());
    }

    /// A tool call, pointed at what it names. A path inside the directory the
    /// agent was working in reads the way the agent would say it —
    /// `app/src/doc.rs`, not the whole of it from the root.
    fn call(&mut self, name: Option<&str>, input: &Value, id: Option<&str>, cwd: Option<&str>) {
        let target = crate::mcp_tail::subject(input);
        let target = match cwd
            .and_then(|c| target.strip_prefix(c))
            .and_then(|rest| rest.strip_prefix('/'))
        {
            Some(inside) if !inside.is_empty() => inside.to_string(),
            _ => target,
        };
        self.push(Entry::Tool(ToolCall {
            name: crate::toolprop::bare(name.unwrap_or("?")).to_string(),
            target,
            result: None,
        }));
        if let Some(id) = id {
            self.tools.insert(id.to_string(), self.entries.len() - 1);
        }
    }

    /// A Claude Code tool result, onto the call it answers.
    fn result(&mut self, b: &Value) {
        let (text, pictures) = result_text(b.get("content"));
        let error = b.get("is_error").and_then(Value::as_bool) == Some(true);
        self.answer(
            b.get("tool_use_id").and_then(Value::as_str),
            &text,
            pictures,
            error,
        );
    }

    /// What call `id` printed, onto its row. A result for a call never seen —
    /// the file was read from its middle — is dropped.
    fn answer(&mut self, id: Option<&str>, text: &str, pictures: usize, error: bool) {
        let Some(at) = id.and_then(|id| self.tools.get(id).copied()) else {
            return;
        };
        if let Some(Entry::Tool(call)) = self.entries.get_mut(at) {
            call.result = Some(tool_out(&call.name, text, pictures, error));
            self.rev += 1;
        }
    }
}

/// What a person typed, from a Codex user message. Codex files its own
/// instructions — AGENTS.md, the environment, plugin lists — as user messages
/// too, and marks every block with what it is; a person's are `user.*`
/// (measured on this machine's rollouts on 2026-09-29: the marks were present and
/// block for block on all 309 user messages). A pasted picture arrives wrapped
/// in `<image name=…>` / `</image>` and reads as `[image]`. Without marks, a
/// message that opens with one of Codex's own wrappers is not typing.
fn codex_typed(p: &Value) -> Option<String> {
    let blocks = p.get("content")?.as_array()?;
    let marks: Option<Vec<&str>> = p
        .pointer("/internal_chat_message_metadata_passthrough/content_item_kinds")
        .and_then(Value::as_array)
        .map(|k| k.iter().map(|m| m.as_str().unwrap_or_default()).collect());
    let mut text = String::new();
    let mut pictures = 0;
    for (i, b) in blocks.iter().enumerate() {
        let theirs = match marks.as_ref() {
            Some(marks) => marks.get(i).is_some_and(|m| m.starts_with("user.")),
            None => true,
        };
        if !theirs {
            continue;
        }
        match b.get("type").and_then(Value::as_str) {
            Some("input_image") => pictures += 1,
            Some("input_text") => {
                let t = b.get("text").and_then(Value::as_str).unwrap_or_default();
                let head = t.trim_start();
                if head.starts_with("<image name=") || head.trim_end() == "</image>" {
                    continue;
                }
                if marks.is_none() && CODEX_OWN.iter().any(|w| head.starts_with(w)) {
                    return None;
                }
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(t);
            }
            _ => {}
        }
    }
    for _ in 0..pictures {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str("[image]");
    }
    (!text.trim().is_empty()).then_some(text)
}

/// How Codex's own user messages open — its instructions, the environment it
/// runs in, the plugins it offers, the note that a turn was cut short — for a
/// rollout old enough to carry no marks.
const CODEX_OWN: [&str; 6] = [
    "# AGENTS.md instructions",
    "<INSTRUCTIONS>",
    "<user_instructions>",
    "<environment_context>",
    "<recommended_plugins>",
    "<turn_aborted>",
];

/// A Codex `exec` result, out of its wrapper: Codex opens what a script
/// printed with "Script completed" or "Script failed" and the wall time, then
/// "Output:". Answers what the script printed and whether it failed; anything
/// else comes back whole.
fn unwrapped(text: &str) -> (&str, bool) {
    let first = text.lines().next().unwrap_or_default().trim();
    let failed = first == "Script failed";
    if !(failed || first == "Script completed") {
        return (text, false);
    }
    match text.find("\nOutput:\n") {
        Some(at) => (&text[at + "\nOutput:\n".len()..], failed),
        None => (text, failed),
    }
}

/// Prompts Claude Code writes that nobody typed, by the tag they open with: a
/// background task's notification, and the output of a local slash command or
/// of a `!` command.
const NOT_TYPED: [&str; 5] = [
    "<task-notification>",
    "<local-command-stdout>",
    "<local-command-stderr>",
    "<bash-stdout>",
    "<bash-stderr>",
];

/// What a person typed, from a prompt's content — a string, or text and image
/// blocks joined, each picture a placeholder — or `None` when nobody typed it.
///
/// Claude Code files more than typing as a person's prompt. Measured over this
/// machine's last 400 transcripts on 2026-09-29: beside 1,672 typed prompts sat
/// 392 task notifications and 171 command outputs, which would otherwise be
/// drawn under YOU as the person's own words. A slash command is stored as the
/// tags around it and is shown as it was typed — `/effort max` — and a `!`
/// command likewise. Pasted text keeps its `<pasted_content>` wrapper: it is the
/// person's, and the wrapper says how it arrived.
fn typed(content: &Value) -> Option<String> {
    let mut text = String::new();
    let mut pictures = 0;
    match content {
        Value::String(s) => text.push_str(s),
        Value::Array(blocks) => {
            for b in blocks {
                match b.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(t) = b.get("text").and_then(Value::as_str) {
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(t);
                        }
                    }
                    Some("image") => pictures += 1,
                    _ => {}
                }
            }
        }
        _ => return None,
    }
    let head = text.trim_start();
    if NOT_TYPED.iter().any(|tag| head.starts_with(tag)) {
        return None;
    }
    if head.starts_with("<command-message>") || head.starts_with("<command-name>") {
        if let Some(name) = tagged(head, "command-name") {
            let args = tagged(head, "command-args").unwrap_or_default();
            text = format!("{name} {args}").trim_end().to_string();
        }
    } else if head.starts_with("<bash-input>") {
        if let Some(command) = tagged(head, "bash-input") {
            text = format!("! {command}");
        }
    }
    for _ in 0..pictures {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str("[image]");
    }
    (!text.trim().is_empty()).then_some(text)
}

/// The text inside the first `<tag>…</tag>` in `s`, trimmed.
fn tagged<'a>(s: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let from = s.find(&open)? + open.len();
    let to = from + s[from..].find(&format!("</{tag}>"))?;
    Some(s[from..to].trim())
}

/// A result's or a message's text and how many pictures it carried: the
/// content is a string, or a list of text and image blocks — Claude Code's
/// `text` and `image`, Codex's `input_text`, `output_text` and `input_image`.
fn result_text(content: Option<&Value>) -> (String, usize) {
    match content {
        Some(Value::String(s)) => (s.clone(), 0),
        Some(Value::Array(blocks)) => {
            let mut text = String::new();
            let mut pictures = 0;
            for b in blocks {
                match b.get("type").and_then(Value::as_str) {
                    Some("text" | "input_text" | "output_text") => {
                        if let Some(t) = b.get("text").and_then(Value::as_str) {
                            if !text.is_empty() && !text.ends_with('\n') {
                                text.push('\n');
                            }
                            text.push_str(t);
                        }
                    }
                    Some("image" | "input_image") => pictures += 1,
                    _ => {}
                }
            }
            (text, pictures)
        }
        _ => (String::new(), 0),
    }
}

/// What a tool's row says it printed, and what opening it shows.
fn tool_out(name: &str, text: &str, pictures: usize, error: bool) -> ToolOut {
    let lines: Vec<&str> = text.lines().collect();
    // The first line that says something: JSON's lone opening brace does not.
    let said = |l: &&str| l.chars().any(char::is_alphanumeric);
    let first = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .find(said)
        .or_else(|| lines.iter().map(|l| l.trim()).find(|l| !l.is_empty()))
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    let summary = if error {
        first
    } else if name == "Read" && !lines.is_empty() {
        match lines.len() {
            1 => "1 line".to_string(),
            n => format!("{n} lines"),
        }
    } else if first.is_empty() && pictures > 0 {
        match pictures {
            1 => "an image".to_string(),
            n => format!("{n} images"),
        }
    } else {
        first
    };
    let mut kept = String::new();
    let mut taken = 0;
    for l in &lines {
        if taken == KEEP_LINES || kept.len() + l.len() > KEEP_BYTES {
            break;
        }
        kept.push_str(l);
        kept.push('\n');
        taken += 1;
    }
    ToolOut {
        summary,
        text: kept.trim_end().to_string(),
        cut: lines.len() - taken,
        error,
    }
}

// ── drawing it ──────────────────────────────────────────────────────────────

/// The pane's font and palette, in the roles the reader's first brief drew:
/// prose and headings in the text colour, headings bold; what you typed in the
/// theme's `human` colour, which exists so your turns stand out from the
/// agent's; results and rules faint; markers and labels in the accent.
#[derive(Clone)]
pub struct Ink {
    pub font: Font,
    pub text: Hsla,
    pub human: Hsla,
    pub faint: Hsla,
    pub accent: Hsla,
    pub error: Hsla,
}

/// What a run is drawn as.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    Text,
    You,
    Bold,
    Italic,
    Code,
    Dim,
    DimBold,
    Accent,
    Label,
    Error,
}

impl Ink {
    fn run(&self, len: usize, role: Role) -> TextRun {
        let mut font = self.font.clone();
        let color = match role {
            Role::Text | Role::Italic | Role::Bold => self.text,
            Role::You => self.human,
            Role::Code | Role::Accent | Role::Label => self.accent,
            Role::Dim | Role::DimBold => self.faint,
            Role::Error => self.error,
        };
        if matches!(role, Role::Bold | Role::DimBold | Role::Label) {
            font.weight = FontWeight::BOLD;
        }
        if role == Role::Italic {
            font.style = FontStyle::Italic;
        }
        TextRun {
            len,
            font,
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }
}

/// One logical line being built from styled pieces.
struct Line<'a> {
    ink: &'a Ink,
    text: String,
    runs: Vec<TextRun>,
}

impl<'a> Line<'a> {
    fn new(ink: &'a Ink) -> Self {
        Self {
            ink,
            text: String::new(),
            runs: Vec::new(),
        }
    }

    fn push(mut self, s: &str, role: Role) -> Self {
        if !s.is_empty() {
            self.text.push_str(s);
            self.runs.push(self.ink.run(s.len(), role));
        }
        self
    }

    fn done(self) -> DocLine {
        DocLine::new(self.text, self.runs)
    }
}

/// A conversation laid out for the reader: its document, and for each of the
/// document's lines the tool call a click on it opens or closes.
pub struct Drawn {
    pub doc: Document,
    pub opens: Vec<Option<usize>>,
}

/// The words a drawn conversation carries, in the reader's language.
pub struct Words<'a> {
    /// Over what the person typed.
    pub you: &'a str,
    /// Over the agent's side: the pane's own label, CLAUDE or CODEX.
    pub agent: &'a str,
    /// In place of a conversation with nothing in it yet — a Codex session
    /// opens its rollout before anything is typed.
    pub empty: &'a str,
}

/// Lay `conv` out as the reader's document. `open` holds the tool calls, by
/// entry, whose output is shown.
pub fn draw(conv: &Conversation, ink: &Ink, words: &Words, open: &HashSet<usize>) -> Drawn {
    let mut out = Drawn {
        doc: Document::default(),
        opens: Vec::new(),
    };
    fn push(out: &mut Drawn, line: DocLine, opens: Option<usize>) {
        out.doc.lines.push(line);
        out.opens.push(opens);
    }
    // A bound conversation with nothing in it says so, rather than leaving a
    // blank glass under a chip that says it is live.
    if conv.entries().is_empty() {
        push(
            &mut out,
            Line::new(ink).push(words.empty, Role::Dim).done(),
            None,
        );
        return out;
    }
    let mut last: Option<&Entry> = None;
    for (i, entry) in conv.entries().iter().enumerate() {
        let theirs = matches!(entry, Entry::Reply(_) | Entry::Tool(_));
        let label = match (entry, last) {
            (Entry::Prompt(_), Some(Entry::Prompt(_))) => None,
            (Entry::Prompt(_), _) => Some(words.you),
            (_, Some(Entry::Reply(_) | Entry::Tool(_))) if theirs => None,
            _ if theirs => Some(words.agent),
            _ => None,
        };
        let joined = matches!((entry, last), (Entry::Tool(_), Some(Entry::Tool(_))));
        if last.is_some() && !joined {
            push(&mut out, DocLine::default(), None);
        }
        if let Some(label) = label {
            push(
                &mut out,
                Line::new(ink).push(label, Role::Label).done(),
                None,
            );
        }
        match entry {
            Entry::Prompt(text) => {
                for l in text.lines() {
                    push(&mut out, Line::new(ink).push(l, Role::You).done(), None);
                }
            }
            Entry::Reply(md) => {
                for line in markdown_lines(md, ink) {
                    push(&mut out, line, None);
                }
            }
            Entry::Tool(call) => {
                push(&mut out, tool_line(call, ink), Some(i));
                // Opened, a call shows what its row left out of the command,
                // then what it printed.
                if open.contains(&i) && row_cuts(&call.target) {
                    for l in call.target.lines() {
                        push(
                            &mut out,
                            Line::new(ink)
                                .push("      ", Role::Dim)
                                .push(l, Role::Text)
                                .done(),
                            Some(i),
                        );
                    }
                }
                if let (true, Some(r)) = (open.contains(&i), call.result.as_ref()) {
                    for l in r.text.lines() {
                        push(
                            &mut out,
                            Line::new(ink)
                                .push("      ", Role::Dim)
                                .push(l, Role::Dim)
                                .done(),
                            Some(i),
                        );
                    }
                    if r.cut > 0 {
                        let more = format!("      … {} more lines", r.cut);
                        push(
                            &mut out,
                            Line::new(ink).push(&more, Role::Dim).done(),
                            Some(i),
                        );
                    }
                }
            }
            Entry::Compacted => {
                push(
                    &mut out,
                    Line::new(ink)
                        .push("───  the conversation was compacted here  ───", Role::Dim)
                        .done(),
                    None,
                );
            }
        }
        last = Some(entry);
    }
    out
}

/// A click on `line` of a drawn conversation: the tool call the line belongs
/// to — its row, or a row of its output — is opened, or put away if it was
/// open. Answers whether the line belonged to a call at all.
pub fn toggle(open: &mut HashSet<usize>, opens: &[Option<usize>], line: usize) -> bool {
    let Some(entry) = opens.get(line).copied().flatten() else {
        return false;
    };
    if !open.remove(&entry) {
        open.insert(entry);
    }
    true
}

/// A tool call's row: `⚙ Name  target  →  what it printed`, the result dim, or
/// the error in the error colour; a call still running has no arrow yet.
fn tool_line(call: &ToolCall, ink: &Ink) -> DocLine {
    let mut line = Line::new(ink)
        .push("⚙ ", Role::Accent)
        .push(&format!("{:<6}", call.name), Role::Bold);
    if !call.target.is_empty() {
        line = line
            .push(" ", Role::Text)
            .push(&one_line(&call.target, TARGET_MAX), Role::Text);
    }
    if let Some(r) = &call.result {
        if !r.summary.is_empty() {
            let role = if r.error { Role::Error } else { Role::Dim };
            line = line
                .push("  →  ", Role::Dim)
                .push(&one_line(&r.summary, SUMMARY_MAX), role);
        }
    }
    line.done()
}

/// How much of a call's target and result its row draws. With `⚙ `, a name of
/// up to nineteen letters and the arrow, they fit the 175 columns a reader
/// opens with in a 1,576-pixel window, so a call stays one row; what the row
/// leaves out is under the click.
const TARGET_MAX: usize = 88;
const SUMMARY_MAX: usize = 60;

/// `s` on one line, its whitespace collapsed, cut to `max` characters with an
/// ellipsis when it is longer.
fn one_line(s: &str, max: usize) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > max {
        one.chars().take(max - 1).collect::<String>() + "…"
    } else {
        one
    }
}

/// Did a call's row leave part of its target out — cut short, or folded from
/// several lines onto one?
fn row_cuts(target: &str) -> bool {
    target.contains('\n') || target.chars().count() > TARGET_MAX
}

/// A reply's Markdown as the reader's lines, read through the document view's
/// own parser.
fn markdown_lines(md: &str, ink: &Ink) -> Vec<DocLine> {
    let parsed = markdown::parse(md, None);
    let mut out = Vec::new();
    for (i, block) in parsed.blocks.iter().enumerate() {
        if i > 0 {
            out.push(DocLine::default());
        }
        block_lines(block, ink, "", &mut out);
    }
    out
}

/// A new line starting with `lead`, the gutter of whatever the block sits in.
fn led<'a>(ink: &'a Ink, lead: &str) -> Line<'a> {
    Line::new(ink).push(lead, Role::Dim)
}

fn block_lines(block: &Block, ink: &Ink, lead: &str, out: &mut Vec<DocLine>) {
    match block {
        Block::Heading { inline, .. } => {
            out.push(led(ink, lead).push(inline.text(), Role::Bold).done());
        }
        Block::Paragraph(inline) => out.push(inline_line(led(ink, lead), inline).done()),
        Block::Code(lines) => {
            for l in lines {
                out.push(
                    led(ink, lead)
                        .push("│ ", Role::Dim)
                        .push(l, Role::Text)
                        .done(),
                );
            }
        }
        Block::Quote(blocks) => {
            let deeper = format!("{lead}│ ");
            for (i, b) in blocks.iter().enumerate() {
                if i > 0 {
                    out.push(led(ink, lead).push("│", Role::Dim).done());
                }
                block_lines(b, ink, &deeper, out);
            }
        }
        Block::List(items) => {
            for (marker, blocks) in items {
                let head = format!("{lead}{marker} ");
                let hang = format!("{lead}{}", " ".repeat(marker.chars().count() + 1));
                for (j, b) in blocks.iter().enumerate() {
                    match (j, b) {
                        (0, Block::Paragraph(inline)) => out.push(
                            inline_line(
                                Line::new(ink)
                                    .push(lead, Role::Dim)
                                    .push(&head[lead.len()..], Role::Accent),
                                inline,
                            )
                            .done(),
                        ),
                        (0, other) => {
                            out.push(Line::new(ink).push(&head, Role::Accent).done());
                            block_lines(other, ink, &hang, out);
                        }
                        (_, other) => block_lines(other, ink, &hang, out),
                    }
                }
            }
        }
        Block::Table(rows) => table_lines(rows, ink, lead, out),
        Block::Rule => out.push(led(ink, lead).push(&"─".repeat(40), Role::Dim).done()),
        Block::Html(lines) => {
            for l in lines {
                out.push(led(ink, lead).push(l, Role::Dim).done());
            }
        }
        Block::Image { alt, .. } => {
            let s = if alt.is_empty() {
                "[image]".to_string()
            } else {
                format!("[image: {alt}]")
            };
            out.push(led(ink, lead).push(&s, Role::Dim).done());
        }
    }
}

/// An inline run of Markdown onto `line`: bold heavier, italic slanted, code
/// and links in the accent. A run that starts inside one already drawn — a bold
/// word inside a link — draws only its part not yet drawn.
fn inline_line<'a>(mut line: Line<'a>, inline: &Inline) -> Line<'a> {
    let text = inline.text();
    let mut at = 0;
    for (range, e) in inline.runs() {
        let start = range.start.max(at);
        if range.end <= start {
            continue;
        }
        if start > at {
            line = line.push(&text[at..start], Role::Text);
        }
        let role = if e.is_code() || e.is_link() {
            Role::Code
        } else if e.is_bold() {
            Role::Bold
        } else if e.is_italic() {
            Role::Italic
        } else {
            Role::Text
        };
        line = line.push(&text[start..range.end], role);
        at = range.end;
    }
    if at < text.len() {
        line = line.push(&text[at..], Role::Text);
    }
    line
}

/// A table as aligned text: each column as wide as its widest cell, the header
/// row in dim capitals over a rule, four spaces between columns.
fn table_lines(rows: &[(bool, Vec<Inline>)], ink: &Ink, lead: &str, out: &mut Vec<DocLine>) {
    let cols = rows.iter().map(|(_, cells)| cells.len()).max().unwrap_or(0);
    let width = |c: usize| {
        rows.iter()
            .filter_map(|(_, cells)| cells.get(c))
            .map(|cell| cell.text().chars().count())
            .max()
            .unwrap_or(0)
    };
    let widths: Vec<usize> = (0..cols).map(width).collect();
    for (k, (header, cells)) in rows.iter().enumerate() {
        let mut line = Line::new(ink).push(lead, Role::Dim);
        for (c, w) in widths.iter().enumerate() {
            let cell = cells.get(c).map(Inline::text).unwrap_or("");
            let shown = if *header {
                cell.to_uppercase()
            } else {
                cell.to_string()
            };
            // The last column is not padded: a line ends where its text does.
            let pad = if c + 1 < cols {
                w.saturating_sub(shown.chars().count()) + 4
            } else {
                0
            };
            let role = if *header { Role::DimBold } else { Role::Text };
            line = line.push(&shown, role).push(&" ".repeat(pad), Role::Text);
        }
        out.push(line.done());
        if *header && k + 1 < rows.len() {
            let rule: String = widths
                .iter()
                .enumerate()
                .map(|(c, w)| {
                    let gap = if c + 1 < cols { 4 } else { 0 };
                    format!("{}{}", "─".repeat(*w), " ".repeat(gap))
                })
                .collect();
            out.push(
                Line::new(ink)
                    .push(lead, Role::Dim)
                    .push(rule.trim_end(), Role::Dim)
                    .done(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // Record shapes cut from a real Claude Code transcript (2026-09-29), each
    // trimmed to the fields the fold reads plus a few it must pass over.
    const PROMPT: &str = r#"{"type":"user","isSidechain":false,"message":{"role":"user","content":"The Alt plus R hotkey to open up the reader."},"uuid":"367e0f36","timestamp":"2026-09-29T16:02:11.000Z"}"#;
    const META: &str = r#"{"type":"user","isMeta":true,"isSidechain":false,"message":{"role":"user","content":"[Image: original 1459x2107, displayed at 1385x2000.]"}}"#;
    const THINKING: &str = r#"{"type":"assistant","isSidechain":false,"message":{"id":"msg_01A","role":"assistant","content":[{"type":"thinking","thinking":"Let me look at the reader first."}]}}"#;
    const TEXT_1: &str = r#"{"type":"assistant","isSidechain":false,"message":{"id":"msg_01A","role":"assistant","content":[{"type":"text","text":"The reader is only as wide as the pane."}]}}"#;
    const TEXT_2: &str = r#"{"type":"assistant","isSidechain":false,"message":{"id":"msg_01A","role":"assistant","content":[{"type":"text","text":"It draws the pane's own columns."}]}}"#;
    const READ: &str = r#"{"type":"assistant","isSidechain":false,"message":{"id":"msg_01B","role":"assistant","content":[{"type":"tool_use","id":"toolu_01R","name":"Read","input":{"file_path":"app/src/doc.rs"}}]}}"#;
    const READ_OUT: &str = r#"{"type":"user","isSidechain":false,"message":{"role":"user","content":[{"tool_use_id":"toolu_01R","type":"tool_result","content":"     1\t//! The reader's document model\n     2\t\n     3\tuse gpui::TextRun;"}]},"toolUseResult":{"type":"text"}}"#;
    const BASH: &str = r#"{"type":"assistant","isSidechain":false,"message":{"id":"msg_01C","role":"assistant","content":[{"type":"tool_use","id":"toolu_01B","name":"mcp__lean-ctx__ctx_shell","input":{"command":"stty -F /proc/3431657/fd/0 size"}}]}}"#;
    const BASH_ERR: &str = r#"{"type":"user","isSidechain":false,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_01B","is_error":true,"content":[{"type":"text","text":"Exit code 1\nstty: /proc/3431657/fd/0: No such file"}]}]}}"#;
    const COMPACT: &str = r#"{"type":"system","subtype":"compact_boundary","content":"Conversation compacted","compactMetadata":{"trigger":"auto"}}"#;
    const SUMMARY: &str = r#"{"type":"user","isCompactSummary":true,"isSidechain":false,"message":{"role":"user","content":"This session is being continued from a previous conversation."}}"#;
    const SIDECHAIN: &str = r#"{"type":"assistant","isSidechain":true,"message":{"id":"msg_09","role":"assistant","content":[{"type":"text","text":"A subagent's own reply."}]}}"#;
    const ATTACHMENT: &str =
        r#"{"type":"attachment","attachment":{"type":"hook_success","content":"ok"}}"#;
    const NOTICE: &str = r#"{"type":"attachment","attachment":{"type":"queued_command","commandMode":"task-notification","prompt":"<task-notification>\n<task-id>ab5c</task-id>"}}"#;
    const MODE: &str = r#"{"type":"permission-mode","permissionMode":"acceptEdits"}"#;

    fn folded(lines: &[&str]) -> Conversation {
        let mut c = Conversation::new();
        for l in lines {
            c.fold(l);
        }
        c
    }

    /// What a person reads, and only that: the prompt, one reply made of the
    /// message's two text blocks, the two tool calls with their results, and a
    /// divider for the compaction — never the thinking, the injected image
    /// marker, the summary, a subagent's reply, a hook, a notification or a mode.
    #[test]
    fn a_transcript_folds_into_what_a_person_reads() {
        let c = folded(&[
            PROMPT, META, THINKING, TEXT_1, TEXT_2, READ, READ_OUT, BASH, BASH_ERR, COMPACT,
            SUMMARY, SIDECHAIN, ATTACHMENT, NOTICE, MODE,
        ]);
        assert_eq!(
            c.entries(),
            &[
                Entry::Prompt("The Alt plus R hotkey to open up the reader.".into()),
                Entry::Reply(
                    "The reader is only as wide as the pane.\n\nIt draws the pane's own columns."
                        .into()
                ),
                Entry::Tool(ToolCall {
                    name: "Read".into(),
                    target: "app/src/doc.rs".into(),
                    result: Some(ToolOut {
                        summary: "3 lines".into(),
                        text: "     1\t//! The reader's document model\n     2\t\n     3\tuse gpui::TextRun;".into(),
                        cut: 0,
                        error: false,
                    }),
                }),
                Entry::Tool(ToolCall {
                    name: "ctx_shell".into(),
                    target: "stty -F /proc/3431657/fd/0 size".into(),
                    result: Some(ToolOut {
                        summary: "Exit code 1".into(),
                        text: "Exit code 1\nstty: /proc/3431657/fd/0: No such file".into(),
                        cut: 0,
                        error: true,
                    }),
                }),
                Entry::Compacted,
            ]
        );
    }

    /// Two messages' text are two replies, however close together: joining is
    /// by the message a block came from, never by being next to each other.
    #[test]
    fn replies_join_by_message_and_nothing_else() {
        let other = TEXT_2.replace("msg_01A", "msg_02Z");
        let c = folded(&[TEXT_1, &other]);
        assert_eq!(c.entries().len(), 2, "{:?}", c.entries());
        let c = folded(&[TEXT_1, READ, TEXT_2]);
        assert_eq!(
            c.entries().len(),
            3,
            "a tool call between two blocks of one message keeps them apart, in reading order"
        );
    }

    /// Claude Code files things nobody typed as a person's prompt — a background
    /// task's notification, a local command's output — and those stay out; a
    /// slash command and a `!` command read as they were typed; a prompt queued
    /// while the agent was busy is kept with its picture, and pasted text with
    /// its wrapper; a queued notification or message from another agent is not.
    /// Every shape here was found in this machine's transcripts (2026-09-29).
    #[test]
    fn only_what_a_person_typed_is_a_prompt() {
        let c = folded(&[
            r#"{"type":"user","message":{"role":"user","content":"<task-notification>\n<task-id>b1k09y0li</task-id>\n<status>completed</status>\n</task-notification>"}}"#,
            r#"{"type":"user","message":{"role":"user","content":"<local-command-stdout>Set effort level to max</local-command-stdout>"}}"#,
            r#"{"type":"user","message":{"role":"user","content":"<command-message>effort</command-message>\n<command-name>/effort</command-name>\n<command-args>max</command-args>"}}"#,
            r#"{"type":"user","message":{"role":"user","content":"<command-name>/clear</command-name>\n<command-message>clear</command-message>\n<command-args></command-args>"}}"#,
            r#"{"type":"user","message":{"role":"user","content":"<bash-input>git status</bash-input>"}}"#,
            r#"{"type":"user","message":{"role":"user","content":"<bash-stdout>On branch main</bash-stdout><bash-stderr></bash-stderr>"}}"#,
            r#"{"type":"attachment","attachment":{"type":"queued_command","commandMode":"prompt","prompt":"<pasted_content id=\"2f11\">\nnotes\n</pasted_content>"}}"#,
            r#"{"type":"attachment","attachment":{"type":"queued_command","commandMode":"prompt","prompt":[{"type":"text","text":"and this one"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":""}}]}}"#,
            r#"{"type":"attachment","attachment":{"type":"queued_command","commandMode":"prompt","isMeta":true,"prompt":"<cross-session-message from=\"uds:/run/user/1000/cc-socks/413\">hello</cross-session-message>"}}"#,
            NOTICE,
        ]);
        assert_eq!(
            c.entries(),
            &[
                Entry::Prompt("/effort max".into()),
                Entry::Prompt("/clear".into()),
                Entry::Prompt("! git status".into()),
                Entry::Prompt("<pasted_content id=\"2f11\">\nnotes\n</pasted_content>".into()),
                Entry::Prompt("and this one\n[image]".into()),
            ]
        );
    }

    /// A call is one row, however long its command or its first line of output:
    /// the row cuts both to fit a reader of the default width, and opening the
    /// call shows the whole command, as written, over the whole output.
    #[test]
    fn a_long_call_is_one_row_and_the_rest_is_behind_the_click() {
        let command = format!("cd /tmp && {}\necho done", "x".repeat(140));
        let printed = format!("{{\n{}\n}}", "y".repeat(150));
        let call = serde_json::json!({"type":"assistant","message":{"id":"m","content":[
            {"type":"tool_use","id":"t","name":"ctx_shell","input":{"command":command}}]}})
        .to_string();
        let out = serde_json::json!({"type":"user","message":{"role":"user","content":[
            {"tool_use_id":"t","type":"tool_result","content":printed}]}})
        .to_string();
        let c = folded(&[call.as_str(), out.as_str()]);
        let shut = draw(&c, &ink(), &words("CLAUDE"), &HashSet::new());
        let row = &shut.doc.lines.last().expect("the row").text;
        assert!(
            row.chars().count() <= 175,
            "{} columns",
            row.chars().count()
        );
        assert!(
            row.contains("→  yyyy"),
            "summed up by its first line with words in it, not the brace: {row}"
        );
        assert_eq!(row.matches('…').count(), 2, "both cut: {row}");
        let opened = draw(&c, &ink(), &words("CLAUDE"), &[0].into());
        let t: Vec<&str> = texts(&opened).into_iter().map(str::trim).collect();
        assert_eq!(
            &t[2..],
            &[
                format!("cd /tmp && {}", "x".repeat(140)).as_str(),
                "echo done",
                "{",
                "y".repeat(150).as_str(),
                "}",
            ]
        );
        let short = folded(&[READ, READ_OUT]);
        let opened = draw(&short, &ink(), &words("CLAUDE"), &[0].into());
        assert!(
            !texts(&opened).iter().any(|l| l.trim() == "app/src/doc.rs"),
            "a target the row showed whole is not shown twice"
        );
    }

    /// A path inside the directory the agent was in reads from there, as the
    /// agent would say it; a path elsewhere, a sibling sharing the prefix, and a
    /// command that merely mentions the directory are left whole.
    #[test]
    fn a_path_reads_from_the_directory_the_agent_was_in() {
        let target = |input: &str| {
            let line = format!(
                r#"{{"type":"assistant","cwd":"/home/p/td","message":{{"id":"m","content":[{{"type":"tool_use","id":"t","name":"Read","input":{input}}}]}}}}"#
            );
            match &folded(&[&line]).entries()[0] {
                Entry::Tool(call) => call.target.clone(),
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(
            target(r#"{"file_path":"/home/p/td/app/src/doc.rs"}"#),
            "app/src/doc.rs"
        );
        assert_eq!(
            target(r#"{"file_path":"/home/p/tdx/doc.rs"}"#),
            "/home/p/tdx/doc.rs"
        );
        assert_eq!(
            target(r#"{"command":"cd /home/p/td && ls"}"#),
            "cd /home/p/td && ls"
        );
        assert_eq!(target(r#"{"file_path":"/home/p/td"}"#), "/home/p/td");
    }

    /// A call still running has no result, and a result for a call that was
    /// never seen — the file was read from the middle — is dropped.
    #[test]
    fn a_result_finds_its_call_or_nothing() {
        let c = folded(&[READ]);
        assert!(matches!(
            &c.entries()[0],
            Entry::Tool(ToolCall { result: None, .. })
        ));
        let c = folded(&[BASH_ERR]);
        assert!(c.entries().is_empty());
    }

    /// Every change moves the revision, so a drawing is kept only while fresh.
    #[test]
    fn every_change_moves_the_revision() {
        let mut c = Conversation::new();
        let mut last = c.rev();
        for l in [PROMPT, TEXT_1, TEXT_2, READ, READ_OUT] {
            c.fold(l);
            assert!(c.rev() > last, "{l}");
            last = c.rev();
        }
        c.fold(THINKING);
        assert_eq!(c.rev(), last, "and nothing else does");
    }

    /// A tool's long output is kept to a bounded page, with a count of the rest.
    #[test]
    fn a_long_output_is_kept_to_a_page() {
        let body: String = (0..1000).map(|i| format!("line {i}\n")).collect();
        let out = tool_out("Bash", &body, 0, false);
        assert_eq!(out.summary, "line 0");
        assert_eq!(out.text.lines().count(), KEEP_LINES);
        assert_eq!(out.cut, 1000 - KEEP_LINES);
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("td-transcript-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a scratch dir");
        dir.join("session.jsonl")
    }

    fn append(path: &Path, bytes: &str) {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("the transcript");
        f.write_all(bytes.as_bytes()).expect("written");
    }

    /// The tail reads only what was appended, and never half a line: a record
    /// caught mid-write waits for its newline.
    #[test]
    fn the_tail_reads_only_new_whole_lines() {
        let path = scratch("tail");
        let _ = std::fs::remove_file(&path);
        append(&path, "{\"a\":1}\n{\"b\":");
        let mut tail = Tail::new(path.clone());
        let first = tail.poll().expect("read");
        assert_eq!(first.lines, vec!["{\"a\":1}".to_string()]);
        assert!(!first.reset);
        append(&path, "2}\n{\"c\":3}\n");
        let next = tail.poll().expect("read");
        assert_eq!(
            next.lines,
            vec!["{\"b\":2}".to_string(), "{\"c\":3}".to_string()],
            "the held half is finished by the next read"
        );
        assert_eq!(tail.poll().expect("read"), Polled::default(), "nothing new");
        std::fs::remove_file(&path).ok();
    }

    /// A transcript that shrank or was replaced is another conversation, and
    /// the tail says so and reads it from the start.
    #[test]
    fn a_shrunk_or_replaced_transcript_starts_over() {
        let path = scratch("reset");
        let _ = std::fs::remove_file(&path);
        append(&path, "{\"a\":1}\n{\"b\":2}\n");
        let mut tail = Tail::new(path.clone());
        tail.poll().expect("read");
        std::fs::write(&path, "{\"z\":9}\n").expect("rewritten shorter");
        let again = tail.poll().expect("read");
        assert!(again.reset);
        assert_eq!(again.lines, vec!["{\"z\":9}".to_string()]);
        // Replaced by a longer file at the same path: a new inode.
        let other = path.with_extension("next");
        std::fs::write(&other, "{\"y\":1}\n{\"y\":2}\n{\"y\":3}\n").expect("the other");
        std::fs::rename(&other, &path).expect("moved over");
        let replaced = tail.poll().expect("read");
        assert!(replaced.reset, "a new file, though it is longer");
        assert_eq!(replaced.lines.len(), 3);
        std::fs::remove_file(&path).ok();
    }

    fn ink() -> Ink {
        Ink {
            font: gpui::font("Zed Plex Mono"),
            text: gpui::hsla(0., 0., 0.8, 1.),
            human: gpui::hsla(0.55, 0.6, 0.7, 1.),
            faint: gpui::hsla(0., 0., 0.5, 1.),
            accent: gpui::hsla(0.33, 0.8, 0.5, 1.),
            error: gpui::hsla(0.0, 0.8, 0.6, 1.),
        }
    }

    fn words(agent: &str) -> Words<'_> {
        Words {
            you: "YOU",
            agent,
            empty: "Nothing has been said here yet.",
        }
    }

    fn texts(d: &Drawn) -> Vec<&str> {
        d.doc.lines.iter().map(|l| l.text.as_str()).collect()
    }

    /// The conversation drawn: YOU over the prompt, CLAUDE over the agent's
    /// side once per turn, the reply's Markdown as prose, one row per tool call
    /// — `⚙ Name target → result` — and runs that cover every byte.
    #[test]
    fn a_conversation_draws_as_labelled_turns() {
        let c = folded(&[
            PROMPT, TEXT_1, TEXT_2, READ, READ_OUT, BASH, BASH_ERR, COMPACT,
        ]);
        let d = draw(&c, &ink(), &words("CLAUDE"), &HashSet::new());
        assert_eq!(
            texts(&d),
            vec![
                "YOU",
                "The Alt plus R hotkey to open up the reader.",
                "",
                "CLAUDE",
                "The reader is only as wide as the pane.",
                "",
                "It draws the pane's own columns.",
                "",
                "⚙ Read   app/src/doc.rs  →  3 lines",
                "⚙ ctx_shell stty -F /proc/3431657/fd/0 size  →  Exit code 1",
                "",
                "───  the conversation was compacted here  ───",
            ]
        );
        for l in &d.doc.lines {
            let covered: usize = l.runs.iter().map(|r| r.len).sum();
            assert_eq!(covered, l.text.len(), "{:?}", l.text);
        }
        let tool_rows: Vec<Option<usize>> =
            d.opens.iter().copied().filter(Option::is_some).collect();
        assert_eq!(
            tool_rows,
            vec![Some(2), Some(3)],
            "each tool row names its call"
        );
        let de = draw(
            &c,
            &ink(),
            &Words {
                you: "DU",
                ..words("CLAUDE")
            },
            &HashSet::new(),
        );
        assert_eq!(texts(&de)[0], "DU", "the label is the reader's language's");
    }

    /// A tool call opened shows what it printed under its row, and each of
    /// those rows closes it again.
    #[test]
    fn an_opened_call_shows_what_it_printed() {
        let c = folded(&[READ, READ_OUT]);
        let open: HashSet<usize> = [0].into();
        let d = draw(&c, &ink(), &words("CLAUDE"), &open);
        assert_eq!(texts(&d)[1..].len(), 4, "{:?}", texts(&d));
        assert!(texts(&d)[2].contains("//! The reader's document model"));
        assert!(d.opens[1..].iter().all(|o| *o == Some(0)));
    }

    /// A conversation bound but with nothing in it yet — a Codex session opens
    /// its rollout before anything is typed — says so, rather than leaving the
    /// glass blank under a chip that says it is live.
    #[test]
    fn an_empty_conversation_says_so() {
        let c = folded(&[r#"{"type":"session_meta","payload":{"id":"s","cwd":"/home/p"}}"#]);
        assert!(c.entries().is_empty());
        let d = draw(&c, &ink(), &words("CODEX"), &HashSet::new());
        assert_eq!(texts(&d), vec!["Nothing has been said here yet."]);
        assert_eq!(d.opens, vec![None]);
    }

    /// A click on a call's row opens it, and a click on any row of what it
    /// printed puts it away again; a click anywhere else changes nothing.
    #[test]
    fn a_click_opens_a_call_and_a_click_on_its_output_closes_it() {
        let c = folded(&[PROMPT, READ, READ_OUT]);
        let mut open = HashSet::new();
        let shut = draw(&c, &ink(), &words("CLAUDE"), &open);
        let row = shut
            .opens
            .iter()
            .position(Option::is_some)
            .expect("a tool row");
        assert!(!toggle(&mut open, &shut.opens, 0), "the YOU label");
        assert!(open.is_empty());
        assert!(toggle(&mut open, &shut.opens, row));
        let opened = draw(&c, &ink(), &words("CLAUDE"), &open);
        assert_eq!(
            opened.doc.lines.len(),
            shut.doc.lines.len() + 3,
            "the three lines it printed"
        );
        assert!(toggle(&mut open, &opened.opens, row + 2), "one of them");
        assert!(open.is_empty(), "put away");
        assert!(!toggle(&mut open, &opened.opens, opened.opens.len() + 5));
    }

    /// A Markdown table is aligned text: capitals over a rule, every column as
    /// wide as its widest cell.
    #[test]
    fn a_table_is_laid_out_as_aligned_text() {
        let md = "| Pane | Today |\n|---|---|\n| 201 columns | 0.87x |\n| 48 columns | 3.65x |";
        let lines: Vec<String> = markdown_lines(md, &ink())
            .into_iter()
            .map(|l| l.text)
            .collect();
        assert_eq!(
            lines,
            vec![
                "PANE           TODAY",
                "───────────    ─────",
                "201 columns    0.87x",
                "48 columns     3.65x",
            ]
        );
    }

    /// Markdown keeps its shape as text: a heading bold, a list with its
    /// markers, a code block behind a gutter.
    #[test]
    fn markdown_keeps_its_shape_as_text() {
        let md = "## The plan\n\n- one\n- two\n\n```\nlet x = 1;\n```";
        let lines = markdown_lines(md, &ink());
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["The plan", "", "• one", "• two", "", "│ let x = 1;"]
        );
        assert_eq!(
            lines[0].runs[0].font.weight,
            FontWeight::BOLD,
            "the heading is bold"
        );
    }

    /// A Codex rollout folds into the same conversation (`codex.jsonl`, in the
    /// shapes surveyed on this machine's rollouts): the typed prompt and not the
    /// instructions Codex files as user messages, the `exec` call summed up by
    /// what its script printed rather than Codex's wrapper, a failed script in
    /// the error colour, the reply, the compaction, and a pasted picture —
    /// while reasoning, the developer's message and the event stream stay out.
    #[test]
    fn a_codex_rollout_folds_into_the_same_conversation() {
        let c = folded(
            &include_str!("../tests/fixtures/reader/codex.jsonl")
                .lines()
                .collect::<Vec<_>>(),
        );
        let e = c.entries();
        assert_eq!(e.len(), 6, "{e:#?}");
        assert_eq!(
            e[0],
            Entry::Prompt("Why does Alt+R read the shell behind a document pane?".into())
        );
        let Entry::Tool(exec) = &e[1] else {
            panic!("{:?}", e[1])
        };
        assert_eq!(exec.name, "exec");
        assert!(exec.target.starts_with("const r = await tools.shell("));
        let out = exec.result.as_ref().expect("its output");
        assert_eq!(
            out.summary,
            "5ba60ed:app/src/main.rs:15798: const FZ_MIN: f32 = 0.35;"
        );
        assert!(!out.error);
        assert!(!out.text.contains("Wall time"), "{}", out.text);
        let Entry::Tool(wait) = &e[2] else {
            panic!("{:?}", e[2])
        };
        assert_eq!(wait.name, "wait");
        assert!(
            wait.target.contains("\"cell_id\":\"33\""),
            "{}",
            wait.target
        );
        let out = wait.result.as_ref().expect("its output");
        assert!(out.error, "Script failed");
        assert_eq!(out.summary, "Traceback (most recent call last):");
        assert!(matches!(&e[3], Entry::Reply(r) if r.starts_with("## The reader reads the grid")));
        assert_eq!(e[4], Entry::Compacted);
        assert_eq!(e[5], Entry::Prompt("and this one\n[image]".into()));
        let d = draw(&c, &ink(), &words("CODEX"), &HashSet::new());
        assert_eq!(
            texts(&d)[3],
            "CODEX",
            "the agent's side is labelled for its agent"
        );
    }

    /// A rollout from before Codex marked its blocks: its own messages are known
    /// by how they open, and what is left is typing.
    #[test]
    fn an_unmarked_codex_message_is_known_by_how_it_opens() {
        let message = |text: &str| {
            serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user",
                "content":[{"type":"input_text","text":text}]}})
            .to_string()
        };
        let c = folded(&[
            message("<environment_context>\n  <cwd>/home/p</cwd>\n</environment_context>").as_str(),
            message("# AGENTS.md instructions\n\n<INSTRUCTIONS>\nbe kind\n</INSTRUCTIONS>")
                .as_str(),
            message("<turn_aborted> The user interrupted the previous turn on purpose.").as_str(),
            message("what changed?").as_str(),
        ]);
        assert_eq!(c.entries(), &[Entry::Prompt("what changed?".into())]);
    }

    /// Codex names its session's directory once, at the start, where Claude
    /// Code stamps every record; a Codex call's path reads from there all the
    /// same.
    #[test]
    fn a_codex_path_reads_from_the_sessions_directory() {
        let opening = r#"{"type":"session_meta","payload":{"id":"s","cwd":"/home/p/td"}}"#;
        let call = serde_json::json!({"type":"response_item","payload":{"type":"function_call",
            "name":"read_file","call_id":"c","arguments":"{\"path\":\"/home/p/td/app/src/doc.rs\"}"}})
        .to_string();
        let c = folded(&[opening, call.as_str()]);
        assert!(
            matches!(&c.entries()[0], Entry::Tool(t) if t.target == "app/src/doc.rs"),
            "{:?}",
            c.entries()
        );
    }

    /// The conversation the reader's first brief drew, as Claude Code records
    /// it (`app/tests/fixtures/reader/`, which `scripts/reader-check.sh` also
    /// resumes for its photographs), read through the tail and drawn: the
    /// divider, the prompt whole, three tool rows with paths from the agent's
    /// directory, and the reply's two blocks as one, headings and table and all
    /// — then the turn the agent writes while the reader is open.
    #[test]
    fn the_first_briefs_conversation_draws_as_it_drew() {
        let path = scratch("fixture");
        std::fs::write(
            &path,
            include_str!("../tests/fixtures/reader/conversation.jsonl"),
        )
        .expect("the fixture");
        let mut tail = Tail::new(path.clone());
        let mut c = Conversation::new();
        for l in tail.poll().expect("read").lines {
            c.fold(&l);
        }
        let d = draw(&c, &ink(), &words("CLAUDE"), &HashSet::new());
        let t = texts(&d);
        assert_eq!(t[0], "───  the conversation was compacted here  ───");
        assert_eq!(&t[1..3], &["", "YOU"]);
        assert!(
            t[3].starts_with("The Alt plus R hotkey") && t[3].ends_with("developing this HTML."),
            "the prompt whole, as one line for the reader to wrap: {}",
            t[3]
        );
        assert_eq!(&t[4..6], &["", "CLAUDE"]);
        assert_eq!(
            &t[6..9],
            &[
                "⚙ Read   app/src/doc.rs  →  40 lines",
                "⚙ Bash   git grep -n \"const FZ_MIN\" 5ba60ed -- app/src/main.rs  →  5ba60ed:app/src/main.rs:15798: const FZ_MIN: f32 = 0.35;",
                "⚙ Bash   stty -F /proc/3431657/fd/0 size  →  51 48",
            ],
            "a line a call, one after another"
        );
        assert_eq!(
            t.iter().filter(|l| **l == "CLAUDE").count(),
            1,
            "one turn: the reply's two blocks are one reply"
        );
        for heading in [
            "The reader is only as wide as the pane",
            "An agent's reply can't be widened from the screen",
            "Document panes open the shell behind them",
        ] {
            let at = t.iter().position(|l| *l == heading).expect(heading);
            assert_eq!(
                d.doc.lines[at].runs[0].font.weight,
                FontWeight::BOLD,
                "{heading}"
            );
        }
        let table = t
            .iter()
            .position(|l| l.starts_with("PANE "))
            .expect("the table");
        assert_eq!(
            &t[table..table + 5],
            &[
                "PANE           TODAY              INTENDED",
                "───────────    ───────────────    ──────────────",
                "201 columns    0.87x  201 x 53    1.0x  175 x 46",
                "97 columns     1.81x   97 x 25    1.0x  175 x 46",
                "48 columns     3.65x   48 x 12    1.0x  175 x 46",
            ]
        );
        assert!(t
            .iter()
            .any(|l| l.starts_with("│ claude reply: 5 grid rows")));
        for hidden in [
            "Start at the reader's layout",
            "Three findings",
            "continued from a previous conversation",
            "lean-ctx active",
        ] {
            assert!(!t.iter().any(|l| l.contains(hidden)), "{hidden}");
        }

        // The agent writes its next turn while the reader is open.
        let before = (c.rev(), t.len());
        append(&path, include_str!("../tests/fixtures/reader/later.jsonl"));
        for l in tail.poll().expect("read").lines {
            c.fold(&l);
        }
        std::fs::remove_file(&path).ok();
        assert!(c.rev() > before.0);
        let d = draw(&c, &ink(), &words("CLAUDE"), &HashSet::new());
        let t = texts(&d);
        assert_eq!(
            &t[before.1..],
            &[
                "",
                "⚙ Write  reports/2026-09-29-alt-r-reader.html  →  File created successfully at: /home/parker/Work/terminal-de…",
                "",
                "The brief is written and open beside you: the three findings above, each with its figure, and five decisions to concur on.",
            ],
            "the new turn under the old, with no label: the agent is still speaking"
        );
    }
}
