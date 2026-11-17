//! Seeded fuzzing of the parsers that read words from outside this process:
//! JSON-RPC lines on the MCP socket, the far door's messages and replies, the
//! files a person edits by hand (`sharing.toml`, `machines.toml`), a
//! `start_team` call, and the text typed into an agent's terminal.
//!
//! The crate is one binary with no library target, so `cargo fuzz` cannot
//! link these functions. This is the same idea in the test harness: a small
//! seeded generator mutates a corpus of real inputs, and every input must
//! leave each parser standing and keep its stated property. A failure prints
//! the seed and the input, which is all a rerun needs.
//!
//! `cargo test fuzz::` runs a short, fixed pass on every suite. The deep run
//! is `TD_FUZZ_SECS=600 cargo test --release fuzz::deep -- --ignored`, which
//! the scheduled deep-verification workflow runs; `TD_FUZZ_SEED` starts it
//! somewhere else.

use serde_json::{json, Value};

/// xorshift64*: enough to spread inputs, and the same everywhere for a seed.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Pieces a hostile or merely odd input is made of: JSON punctuation,
/// control characters, escapes, multi-byte and zero-width characters,
/// numbers at the edges, and the words these parsers look for.
const ATOMS: &[&str] = &[
    "{",
    "}",
    "[",
    "]",
    "\"",
    ":",
    ",",
    "\\",
    "\\u0000",
    "\\ud800",
    "\n",
    "\r",
    "\t",
    "\u{1b}",
    "\u{1b}[201~",
    "\u{7}",
    "\u{0}",
    "\u{200b}",
    "\u{202e}",
    "é",
    "😀",
    "-",
    "--",
    "'",
    "`",
    "$(",
    "0",
    "-1",
    "18446744073709551615",
    "1e999",
    "null",
    "true",
    "[]",
    "{}",
    "conv:",
    "seat:",
    "pid:",
    "far/census",
    "far/prompt",
    "far/prompt_status",
    "tools/call",
    "prompt_agent",
    "start_team",
    "machine",
    "agent",
    "from",
    "to",
    "prompt",
    "key",
    "reach",
    "host",
    "ssh",
    "[[machine]]",
    "=",
    "\t{",
    "err ",
    "session ",
    "td ",
];

fn mutate(rng: &mut Rng, seed: &str) -> String {
    let mut s: Vec<char> = seed.chars().collect();
    for _ in 0..=rng.below(6) {
        let at = rng.below(s.len() + 1);
        match rng.below(5) {
            0 if !s.is_empty() => {
                let end = (at + rng.below(8)).min(s.len());
                s.drain(at.min(end)..end);
            }
            1 | 2 => {
                let atom = ATOMS[rng.below(ATOMS.len())];
                s.splice(at..at, atom.chars());
            }
            3 if !s.is_empty() => {
                let i = rng.below(s.len());
                s[i] = char::from_u32((rng.next() % 0x2_0000) as u32).unwrap_or('?');
            }
            _ => {
                // Repeat a slice, which is how a length limit is found.
                let end = (at + rng.below(32)).min(s.len());
                let piece: Vec<char> = s[at.min(end)..end].to_vec();
                let times = rng.below(64);
                s.splice(
                    at..at,
                    piece.iter().copied().cycle().take(piece.len() * times),
                );
            }
        }
    }
    s.into_iter().collect()
}

fn corpus() -> Vec<String> {
    let rpc = |method: &str, params: Value| {
        json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string()
    };
    vec![
        rpc("initialize", json!({ "protocolVersion": "2025-06-18" })),
        rpc("tools/list", json!({})),
        rpc(
            "tools/call",
            json!({ "name": "list_panes", "arguments": {} }),
        ),
        rpc(
            "tools/call",
            json!({ "name": "prompt_agent", "arguments": { "machine": "farbox", "agent": "conv:abc", "prompt": "review", "key": "k1" } }),
        ),
        rpc(
            "tools/call",
            json!({ "name": "ask_agent", "arguments": { "agent": "seat:3" } }),
        ),
        rpc(
            "tools/call",
            json!({ "name": "start_team", "arguments": { "name": "crew", "project": "/tmp", "agents": [{ "role": "reviewer", "harness": "claude", "model": "opus", "prompt": "- read it" }] } }),
        ),
        rpc("far/census", json!({ "from": "legion" })),
        rpc(
            "far/prompt",
            json!({ "from": "legion", "sender": "lead", "to": "seat:2", "prompt": "hello", "key": "seat1/k" }),
        ),
        rpc("far/prompt_status", json!({ "from": "legion", "prompt_id": 7 })),
        "4242\t{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"structuredContent\":{\"machine\":\"farbox\",\"session\":\"1\",\"window\":4242,\"mcp\":true,\"ceiling\":\"prompt\",\"agents\":[{\"addr\":\"conv:abc\",\"name\":\"T\",\"harness\":\"CLAUDE\",\"state\":\"idle\",\"reach\":\"prompt\"}]}}}".into(),
        "4242\terr no control socket".into(),
        "[[machine]]\nhost = \"legion\"\nreach = \"ask\"\n".into(),
        "[[machine]]\nname = \"farbox\"\nssh = \"me@far-box\"\n".into(),
        "td /home/me/.local/bin/terminal-delight\nversion 0.5.4\nsession 1 window\nsession 2 \n".into(),
    ]
}

fn check_one(input: &str) {
    // The MCP line handler: any line gets a reply or none, never a panic,
    // and a reply is one JSON object on one line.
    let mut snap = crate::mcp::Snapshot::empty();
    for enabled in [false, true] {
        snap.config.enabled = enabled;
        if let Some(reply) = crate::mcp::handle_line(input, &snap, |_, _| vec![]) {
            assert!(!reply.contains('\n'), "a reply is one line: {reply:?}");
            let v: Value = serde_json::from_str(&reply).expect("a reply is JSON");
            assert!(v.is_object(), "a reply is an object");
        }
        let _ = crate::mcp::requires_snapshot(input);
        let _ = crate::mcp::names_agents(input);
    }

    // A far agent's name parses back to itself.
    if let Some(addr) = crate::farfloor::FarAddr::parse(input) {
        assert_eq!(
            crate::farfloor::FarAddr::parse(&addr.to_string()),
            Some(addr.clone()),
            "{input:?} -> {addr} does not read back"
        );
    }
    let _ = crate::farfloor::parse_reply_line(input);
    let _ = crate::farfloor::parse_census(input);
    let _ = crate::far::parse_probe(input);

    // A sharing file that reads writes back to one that reads the same.
    if let Ok(s) = crate::farfloor::Sharing::parse(input) {
        let back = crate::farfloor::Sharing::parse(&s.to_toml()).expect("written back, it reads");
        assert_eq!(back, s, "sharing.toml does not round-trip: {input:?}");
    }
    let _ = crate::entry::parse_machines(input);

    // The text typed into a terminal keeps no control character but the
    // two a prompt is allowed, and a header stays one line.
    let clean = crate::prompt::clean(input);
    assert!(
        clean
            .chars()
            .all(|c| !c.is_control() || c == '\n' || c == '\t'),
        "clean() kept a control character: {clean:?}"
    );
    let header = crate::prompt::header_far(crate::threshing_floor::AskId(1), input, true);
    assert!(!header.contains('\n') && !header.chars().any(char::is_control));
    let _ = crate::prompt::key_of(Some(input));

    // A start_team call that reads is one a command line can carry.
    if let Ok(v) = serde_json::from_str::<Value>(input) {
        let args = v.pointer("/params/arguments").cloned().unwrap_or(v.clone());
        if let Ok(spec) = crate::team::parse(&args, Some(std::path::Path::new("/tmp"))) {
            assert!(!spec.members.is_empty() && spec.members.len() <= crate::team::MAX_MEMBERS);
            for m in &spec.members {
                assert!(m.role.chars().count() <= crate::team::NAME_MAX);
                for w in [&m.model, &m.effort].into_iter().flatten() {
                    assert!(!w.starts_with('-'), "a flag got through as a word: {w:?}");
                }
            }
        }
    }
}

fn run(seed: u64, budget: std::time::Duration, max: u64) -> u64 {
    let corpus = corpus();
    let mut rng = Rng::new(seed);
    let start = std::time::Instant::now();
    let mut n = 0u64;
    while n < max && start.elapsed() < budget {
        let base = &corpus[rng.below(corpus.len())];
        let input = mutate(&mut rng, base);
        let caught = std::panic::catch_unwind(|| check_one(&input));
        if caught.is_err() {
            panic!("fuzz: seed {seed}, case {n}, input {input:?}");
        }
        n += 1;
    }
    n
}

/// The short pass every suite runs: fixed seed, fixed count.
#[test]
fn fuzz_parsers_survive_a_fixed_pass() {
    for base in corpus() {
        check_one(&base);
    }
    let n = run(1, std::time::Duration::from_secs(60), 3_000);
    assert_eq!(n, 3_000);
}

/// The deep pass, for the scheduled workflow.
#[test]
#[ignore]
fn deep() {
    let secs = std::env::var("TD_FUZZ_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let seed = std::env::var("TD_FUZZ_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(7)
        });
    eprintln!("fuzz: seed {seed}, {secs}s");
    let n = run(seed, std::time::Duration::from_secs(secs), u64::MAX);
    eprintln!("fuzz: {n} cases, seed {seed}, no failure");
}
