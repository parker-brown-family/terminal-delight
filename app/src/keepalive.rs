//! Cache keepalive — hold an idle agent's prompt cache warm from outside it.
//!
//! An agent session's conversation is cached server-side against the exact
//! token prefix the request carries. Reading that cache bills at 0.1x base
//! input; writing it at a one-hour TTL bills at 2.0x. The TTL refreshes every
//! time the cache is read, so a session stays cheap for as long as somebody is
//! talking to it — and the moment an hour of silence passes, the next message
//! pays 2.0x on the whole conversation instead of 0.1x. Measured on this
//! machine across 32,037 assistant turns in thirty days: 87 such cold returns,
//! 24.2M tokens re-written, average gap 5.0 hours. At the rates the two probes
//! below establish that is ~$242, of which ~$230 buys nothing.
//!
//! The fix is a ping that reads the cache before the hour is up. The thing that
//! makes it safe is that **the ping never touches the live pane**. It is a
//! separate, headless, forked process:
//!
//! ```text
//! claude --resume <id> --fork-session --no-session-persistence \
//!        --max-turns 1 -p <noop> --output-format json
//! ```
//!
//! `--fork-session` gives it a new session id, `--no-session-persistence` keeps
//! it off disk entirely. It cannot type into a terminal, cannot answer a
//! permission prompt, cannot be mistaken for the user. Measured 2026-09-03: two
//! forks of one session, five seconds apart, different session ids — the first
//! wrote 44,688 tokens, the second read 54,693 and wrote **zero**. A fork warms
//! an entry that a different session reads for free, which is the whole design.
//!
//! # The invariant that decides whether any of this works
//!
//! The cache is keyed on the prefix, so **the ping must reproduce the live
//! pane's request byte for byte**. This is not a stylistic preference. The
//! first probe run added `--disallowed-tools "*"` on the theory that a keepalive
//! should not be able to do anything; tool definitions are part of the cached
//! prefix, so the prefix shrank 54,882 -> 41,992, missed the cache completely,
//! and cost $0.4200 instead of $0.0276. A keepalive that alters the prefix does
//! not merely fail to save money — it spends 2.0x on a greeting, every hour,
//! and every number it reports looks fine.
//!
//! So: no `--model`, no `--append-system-prompt`, no tool narrowing, and the
//! pane's own cwd, because the working directory reaches the system prompt.
//! `argv` is the single place that shape is written down and
//! `the_ping_never_narrows_the_prefix` is the test that keeps it that way.
//!
//! # It reports on itself
//!
//! Every ping returns its own `usage`, so the feature can be held to account
//! rather than trusted: `read` should be the pane's prefix and `write` should be
//! zero. If write dominates, the cache was already gone. If the total does not
//! match the pane's own last turn, the ping is warming *a different prefix* than
//! the pane will read — working perfectly, saving nothing. [`Effect`] names
//! those apart, because they are indistinguishable from the outside.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

/// Billing multipliers on base input tokens, as the API charges them.
/// Confirmed against dollars 2026-09-03: two invocations of one session
/// differing only in cache state billed $0.4255 (12,992 read + 41,890 write)
/// and $0.0276 (54,882 read + 0 write), which solves to $0.4965/M read and
/// $10.00/M write — exactly the 20:1 these two constants describe.
pub const READ_MULT: f64 = 0.1;
pub const WRITE_MULT: f64 = 2.0;

/// Ping with five minutes to spare. The TTL is one hour from last use, and a
/// sweep that ticks every sixty seconds can be a minute late on top of however
/// long the process takes to answer.
pub const PING_AFTER: Duration = Duration::from_secs(55 * 60);

/// Stop after eight hours parked. Simulated over the same thirty days: an 8h
/// cap bridges 74 of the 87 cold returns for a net +19.4M input-equivalents
/// (~$97), where 12h nets +12.9M, 24h nets -9.0M and never stopping nets
/// -36.5M. The term that turns it negative is sessions that are never resumed
/// — there were 74 of those, holding 17.2M of parked prefix — and they are
/// indistinguishable from a parked one until the moment you give up on them.
pub const DEFAULT_CAP_HOURS: u64 = 8;

/// Below this there is nothing worth buying. A 20k prefix parked for the full
/// eight hours costs ~9 pings x 2k eq to save ~38k eq, which is still positive
/// but well inside the noise of getting any of the above slightly wrong.
pub const MIN_PREFIX: u64 = 20_000;

/// A well-behaved ping answers `ok`. Anything beyond this and the noop was not
/// a noop — worth reporting, because the reply lands in a forked transcript
/// nobody will ever read.
pub const OUT_BUDGET: u64 = 64;

/// The prompt. Deliberately not a greeting: "Hi, just keeping the cache warm"
/// is ambiguous enough that a capable model answers it with four paragraphs
/// about prompt caching, which is output tokens and, on a session with tools,
/// an invitation to go and check something.
pub const NOOP_PROMPT: &str =
    "[td-keepalive] Terminal Delight cache keepalive. No action required, no tools, \
     no files. Reply with exactly: ok";

/// Which cache tier the session is actually being billed at.
///
/// This matters more than it looks. A five-minute TTL inverts the economics
/// completely — a 55-minute ping arrives 50 minutes after the cache died, so
/// every ping is a full 1.25x re-write and the feature becomes a way to spend
/// money on nothing. Claude Code drops to the short tier under usage overage
/// without saying so, which is exactly the kind of silent inversion that runs
/// for a month before anyone checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ttl {
    Hour,
    Short,
    Unknown,
}

/// What a pane's transcript says about the conversation sitting in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Parked {
    /// The session id to resume — the fork's parent.
    pub session: String,
    /// The pane's working directory. Part of the system prompt, therefore part
    /// of the cached prefix, therefore not optional.
    pub cwd: String,
    /// Epoch seconds of the most recent line of any kind. Tool results are
    /// appended as they land, so a pane grinding through a long build looks
    /// busy here rather than idle.
    pub last_activity: i64,
    /// Epoch seconds of the last turn that was actually billed.
    pub last_billed: i64,
    /// `cache_read + cache_creation` on that turn: the size of the entry a
    /// return will either read for 0.1x or rewrite for 2.0x.
    pub prefix: u64,
    pub ttl: Ttl,
}

/// Whether to ping this pane now, and — when not — why not.
///
/// Every refusal is named. A keepalive that silently declines to fire is
/// indistinguishable from one that is working, and the failure it hides is the
/// expensive direction.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Ping,
    /// Inside the TTL still; the cache does not need us yet.
    NotYet {
        secs: i64,
    },
    /// Past the cap. Let it go cold — see [`DEFAULT_CAP_HOURS`].
    Expired {
        parked_hours: f64,
    },
    /// Too small to be worth the arithmetic.
    TooSmall {
        prefix: u64,
    },
    /// Five-minute tier: pinging would cost money rather than save it.
    ShortTtl,
    Disabled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub ping_after: Duration,
    pub cap: Duration,
    pub min_prefix: u64,
    /// Decide and report, but never spawn. The honest way to watch what the
    /// policy would have done before letting it spend anything.
    pub dry_run: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            ping_after: PING_AFTER,
            cap: Duration::from_secs(DEFAULT_CAP_HOURS * 3600),
            min_prefix: MIN_PREFIX,
            dry_run: false,
        }
    }
}

impl Config {
    /// Off unless asked for. A feature that spends money on a timer does not
    /// get to default to on, however good the arithmetic looks.
    pub fn from_env() -> Self {
        let on = |k: &str| {
            std::env::var(k)
                .map(|v| matches!(v.trim(), "1" | "on" | "true" | "yes"))
                .unwrap_or(false)
        };
        let hours = std::env::var("TD_KEEPALIVE_CAP_HOURS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|h| *h > 0 && *h <= 48)
            .unwrap_or(DEFAULT_CAP_HOURS);
        Self {
            enabled: on("TD_KEEPALIVE"),
            cap: Duration::from_secs(hours * 3600),
            dry_run: on("TD_KEEPALIVE_DRY_RUN"),
            ..Self::default()
        }
    }
}

/// The decision, with no I/O in it.
pub fn verdict(p: &Parked, now: i64, cfg: &Config) -> Verdict {
    if !cfg.enabled {
        return Verdict::Disabled;
    }
    if p.ttl == Ttl::Short {
        return Verdict::ShortTtl;
    }
    if p.prefix < cfg.min_prefix {
        return Verdict::TooSmall { prefix: p.prefix };
    }
    let parked = now - p.last_billed;
    if parked > cfg.cap.as_secs() as i64 {
        return Verdict::Expired {
            parked_hours: parked as f64 / 3600.0,
        };
    }
    // Measured against the most recent line of any kind, not the last billed
    // turn: a pane mid-tool-run is still being talked to, and its next turn
    // will refresh the cache for free.
    let idle = now - p.last_activity;
    if idle < cfg.ping_after.as_secs() as i64 {
        return Verdict::NotYet {
            secs: cfg.ping_after.as_secs() as i64 - idle,
        };
    }
    Verdict::Ping
}

/// The invocation. **Every argument here is load-bearing on the cache key.**
/// Adding one that changes the system prompt or the tool set turns this feature
/// from a 20:1 saving into a 2.0x tax — see the module header.
pub fn argv(session: &str) -> Vec<String> {
    let a = args(session);
    // The same invariant the test asserts, kept live in debug builds: this is
    // the one mistake in this module that costs money rather than correctness,
    // and it would be made by someone adding a flag in good faith.
    debug_assert!(
        !a.iter().any(|x| PREFIX_BREAKING.contains(&x.as_str())),
        "a prefix-breaking flag reached the keepalive invocation"
    );
    a
}

fn args(session: &str) -> Vec<String> {
    vec![
        "--resume".into(),
        session.into(),
        // A new session id, so the live pane's transcript is never touched.
        "--fork-session".into(),
        // ...and no transcript of our own. Verified: the project directory's
        // jsonl count was 53 before and 53 after.
        "--no-session-persistence".into(),
        // Bounds the blast radius of a model that decides to be helpful.
        "--max-turns".into(),
        "1".into(),
        "-p".into(),
        NOOP_PROMPT.into(),
        // The point of the whole exercise: the ping reports its own billing.
        "--output-format".into(),
        "json".into(),
    ]
}

/// Arguments that would change the cached prefix, and therefore must never
/// appear in [`argv`]. Named rather than merely avoided so the test can be
/// specific about what it is protecting against.
pub const PREFIX_BREAKING: &[&str] = &[
    "--model",
    "--append-system-prompt",
    "--system-prompt",
    "--allowed-tools",
    "--allowedTools",
    "--disallowed-tools",
    "--disallowedTools",
    "--agent",
    "--permission-mode",
    "--effort",
    "--mcp-config",
    "--setting-sources",
];

/// What the ping's own `--output-format json` said about itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    pub read: u64,
    pub write: u64,
    pub short_ttl: u64,
    pub out: u64,
    pub turns: u64,
    pub cost_usd: f64,
    pub is_error: bool,
}

impl Outcome {
    pub fn total(&self) -> u64 {
        self.read + self.write
    }
    /// Input-token-equivalents this ping actually cost.
    pub fn cost_eq(&self) -> f64 {
        self.read as f64 * READ_MULT + self.write as f64 * WRITE_MULT
    }
}

pub fn parse_outcome(json: &str) -> Option<Outcome> {
    let v: Value = serde_json::from_str(json).ok()?;
    let u = v.get("usage")?;
    let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
    Some(Outcome {
        read: n("cache_read_input_tokens"),
        write: n("cache_creation_input_tokens"),
        short_ttl: u
            .get("cache_creation")
            .and_then(|c| c.get("ephemeral_5m_input_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        out: n("output_tokens"),
        turns: v.get("num_turns").and_then(Value::as_u64).unwrap_or(0),
        cost_usd: v
            .get("total_cost_usd")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        is_error: v
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// How far the ping's prefix may drift from the pane's before we conclude it is
/// warming a different entry. The ping appends its own prompt, so exact equality
/// is not available; 10% is far tighter than the 23% the tool-set mistake moved
/// it and far looser than a turn's ordinary growth.
pub const DRIFT_TOLERANCE: f64 = 0.10;

/// What a ping actually achieved. The three failures are the point: each one
/// looks like success from the outside.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Read the prefix, wrote nothing. The cache is warm and the pane's next
    /// message will read it for 0.1x.
    Warm { read: u64, saved_eq: f64 },
    /// The cache was already gone; this ping paid to recreate it. One of these
    /// is the cost of starting late. A run of them means the cadence is wrong.
    Missed { wrote: u64 },
    /// A clean cache hit — on the wrong prefix. The pane will still pay full
    /// price on return, and nothing else about this ping looks abnormal.
    Drifted { read: u64, expected: u64 },
    /// Five-minute tier. Disable: every ping from here is a pure loss.
    ShortTtl,
    /// The noop answered with more than `ok`, or took a tool turn.
    Chatty { turns: u64, out: u64 },
    /// The invocation itself failed.
    Failed,
}

pub fn effect(o: &Outcome, expected_prefix: u64) -> Effect {
    if o.is_error {
        return Effect::Failed;
    }
    if o.short_ttl > 0 {
        return Effect::ShortTtl;
    }
    if o.turns > 1 || o.out > OUT_BUDGET {
        return Effect::Chatty {
            turns: o.turns,
            out: o.out,
        };
    }
    // Missed before drifted: a ping that rewrote everything has no meaningful
    // read to compare, and calling that "drift" would send the reader looking
    // for a prefix mismatch that is not the problem.
    if o.write > o.total() / 20 {
        return Effect::Missed { wrote: o.write };
    }
    if expected_prefix > 0 {
        let drift = (o.total() as f64 - expected_prefix as f64).abs() / expected_prefix as f64;
        if drift > DRIFT_TOLERANCE {
            return Effect::Drifted {
                read: o.read,
                expected: expected_prefix,
            };
        }
    }
    Effect::Warm {
        read: o.read,
        // What this ping bought: the difference between the cold return it
        // prevents and the warm one it leaves in place, less its own cost.
        saved_eq: o.read as f64 * (WRITE_MULT - READ_MULT) - o.cost_eq(),
    }
}

/// Read a pane's parked state out of the tail of its transcript.
///
/// Only the tail: these files reach hundreds of megabytes and the answer is
/// always in the last few turns.
pub fn parked_from_transcript(path: &Path, session: &str, cwd: &str) -> Option<Parked> {
    let body = read_tail(path, 256 * 1024).ok()?;
    let mut last_activity = 0i64;
    let mut last_billed = 0i64;
    let mut prefix = 0u64;
    let mut ttl = Ttl::Unknown;

    for line in body.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue; // a truncated first line, or a format we do not know
        };
        let Some(ts) = v
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(crate::usage::parse_epoch)
        else {
            continue;
        };
        last_activity = last_activity.max(ts);

        let Some(u) = v
            .get("message")
            .and_then(|m| m.get("usage"))
            .filter(|_| v.get("type").and_then(Value::as_str) == Some("assistant"))
        else {
            continue;
        };
        let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
        let total = n("cache_read_input_tokens") + n("cache_creation_input_tokens");
        if total == 0 {
            continue; // an interrupted or unbilled turn tells us nothing
        }
        if ts >= last_billed {
            last_billed = ts;
            prefix = total;
            ttl = match (
                u.get("cache_creation")
                    .and_then(|c| c.get("ephemeral_5m_input_tokens"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                u.get("cache_creation")
                    .and_then(|c| c.get("ephemeral_1h_input_tokens"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            ) {
                (0, 0) => Ttl::Unknown,
                (short, hour) if short > hour => Ttl::Short,
                _ => Ttl::Hour,
            };
        }
    }

    (last_billed > 0).then(|| Parked {
        session: session.to_string(),
        cwd: cwd.to_string(),
        last_activity,
        last_billed,
        prefix,
        ttl,
    })
}

fn read_tail(path: &Path, bytes: u64) -> std::io::Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let from = len.saturating_sub(bytes);
    f.seek(SeekFrom::Start(from))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    let s = String::from_utf8_lossy(&buf).into_owned();
    // A mid-line start would fail to parse anyway, but dropping it keeps the
    // "unparseable line" path meaning something.
    Ok(if from > 0 {
        s.split_once('\n').map(|(_, rest)| rest).unwrap_or("").into()
    } else {
        s
    })
}

/// One pane's worth of work for the sweep.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub shell_pid: u32,
    pub session: String,
    pub verdict: Verdict,
    pub effect: Option<Effect>,
}

/// Run the policy over every agent pane, ping the ones that qualify, and report
/// what each ping achieved. I/O — call it off the main thread.
pub fn sweep(reqs: &[crate::vitals::PaneReq], home: &Path, cfg: &Config) -> Vec<Report> {
    let mut out = Vec::new();
    let now = crate::usage::now_epoch();
    for r in reqs {
        let (Some(cwd), Some(resume)) = (r.cwd.as_deref(), r.resume.as_deref()) else {
            continue;
        };
        let Some(session) = crate::session::resume_session_id(resume) else {
            continue; // a fresh agent with no session id yet has no cache to hold
        };
        let Some(path) = crate::session::claude_transcript(cwd, Some(resume), home) else {
            continue;
        };
        let Some(parked) = parked_from_transcript(&path, &session, cwd) else {
            continue;
        };
        let v = verdict(&parked, now, cfg);
        let effect = match (&v, cfg.dry_run) {
            (Verdict::Ping, false) => Some(ping(&parked)),
            _ => None,
        };
        out.push(Report {
            shell_pid: r.shell_pid,
            session,
            verdict: v,
            effect,
        });
    }
    out
}

fn ping(p: &Parked) -> Effect {
    let run = std::process::Command::new("claude")
        .args(argv(&p.session))
        // The pane's own directory: it reaches the system prompt, and the
        // system prompt is in the cached prefix.
        .current_dir(&p.cwd)
        .stdin(std::process::Stdio::null())
        .output();
    let Ok(o) = run else {
        return Effect::Failed;
    };
    let Some(outcome) = parse_outcome(&String::from_utf8_lossy(&o.stdout)) else {
        return Effect::Failed;
    };
    effect(&outcome, p.prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn parked(prefix: u64, idle_secs: i64) -> Parked {
        Parked {
            session: "s-1".into(),
            cwd: "/home/parker".into(),
            last_activity: 1_000_000 - idle_secs,
            last_billed: 1_000_000 - idle_secs,
            prefix,
            ttl: Ttl::Hour,
        }
    }
    fn on() -> Config {
        Config {
            enabled: true,
            ..Config::default()
        }
    }
    const NOW: i64 = 1_000_000;

    #[test]
    fn a_pane_inside_the_ttl_is_left_alone() {
        assert!(matches!(
            verdict(&parked(200_000, 40 * 60), NOW, &on()),
            Verdict::NotYet { .. }
        ));
    }

    #[test]
    fn a_pane_past_fifty_five_minutes_is_pinged() {
        assert_eq!(verdict(&parked(200_000, 56 * 60), NOW, &on()), Verdict::Ping);
    }

    #[test]
    fn the_ping_lands_with_time_to_spare_inside_the_hour() {
        // The whole feature rests on arriving before the TTL, not on the hour.
        assert!(PING_AFTER.as_secs() < 3600);
        assert!(3600 - PING_AFTER.as_secs() >= 300);
    }

    #[test]
    fn a_pane_past_the_cap_is_left_to_go_cold() {
        let p = parked(200_000, 9 * 3600);
        assert!(matches!(
            verdict(&p, NOW, &on()),
            Verdict::Expired { .. }
        ));
    }

    #[test]
    fn the_default_cap_is_the_measured_optimum() {
        // Simulated over thirty days of this machine's own transcripts: 8h nets
        // +19.4M input-equivalents, 12h +12.9M, 24h -9.0M. Pinning it here so a
        // later "why not just leave it on" has to argue with the measurement.
        assert_eq!(DEFAULT_CAP_HOURS, 8);
    }

    #[test]
    fn a_pane_still_being_worked_is_not_pinged_even_if_its_last_billed_turn_is_old() {
        // A seventy-minute build appends tool results without billing a turn.
        // The live process will refresh the cache itself when it finishes;
        // paying for a ping alongside it is pure waste.
        let p = Parked {
            last_activity: NOW - 30,
            ..parked(200_000, 70 * 60)
        };
        assert!(matches!(verdict(&p, NOW, &on()), Verdict::NotYet { .. }));
    }

    #[test]
    fn a_five_minute_ttl_refuses_the_ping_because_the_economics_invert() {
        let p = Parked {
            ttl: Ttl::Short,
            ..parked(200_000, 56 * 60)
        };
        assert_eq!(verdict(&p, NOW, &on()), Verdict::ShortTtl);
    }

    #[test]
    fn a_prefix_too_small_to_matter_is_not_worth_a_ping() {
        assert!(matches!(
            verdict(&parked(5_000, 56 * 60), NOW, &on()),
            Verdict::TooSmall { .. }
        ));
    }

    #[test]
    fn the_feature_is_off_unless_asked_for() {
        assert!(!Config::default().enabled);
        assert_eq!(
            verdict(&parked(200_000, 56 * 60), NOW, &Config::default()),
            Verdict::Disabled
        );
    }

    #[test]
    fn the_ping_never_narrows_the_prefix() {
        // The $0.4200 test. `--disallowed-tools "*"` shrank the prefix 54,882 ->
        // 41,992 and missed the cache completely, because tool definitions are
        // cached with the system prompt. Any of these flags does the same.
        let a = argv("s-1");
        for bad in PREFIX_BREAKING {
            assert!(
                !a.iter().any(|x| x == bad),
                "{bad} changes the cached prefix; the ping must reproduce the pane's request exactly"
            );
        }
    }

    #[test]
    fn the_ping_forks_and_leaves_nothing_behind() {
        let a = argv("s-1");
        assert!(a.contains(&"--fork-session".to_string()));
        assert!(a.contains(&"--no-session-persistence".to_string()));
        // ...and it resumes the pane's own session, or it warms nothing.
        let i = a.iter().position(|x| x == "--resume").unwrap();
        assert_eq!(a[i + 1], "s-1");
    }

    #[test]
    fn the_ping_is_byte_identical_across_calls() {
        // The cache key is the prefix. A ping that varies is a ping that pays.
        assert_eq!(argv("s-1"), argv("s-1"));
    }

    #[test]
    fn the_ping_bounds_the_turn_and_asks_for_its_own_billing() {
        let a = argv("s-1");
        let i = a.iter().position(|x| x == "--max-turns").unwrap();
        assert_eq!(a[i + 1], "1");
        let j = a.iter().position(|x| x == "--output-format").unwrap();
        assert_eq!(a[j + 1], "json");
    }

    const WARM: &str = r#"{"subtype":"success","is_error":false,"num_turns":1,"result":"ok",
        "usage":{"cache_read_input_tokens":54882,"cache_creation_input_tokens":0,
        "cache_creation":{"ephemeral_1h_input_tokens":0,"ephemeral_5m_input_tokens":0},
        "output_tokens":4},"total_cost_usd":0.027551}"#;
    const COLD: &str = r#"{"subtype":"success","is_error":false,"num_turns":1,"result":"ok",
        "usage":{"cache_read_input_tokens":12992,"cache_creation_input_tokens":41890,
        "cache_creation":{"ephemeral_1h_input_tokens":41890,"ephemeral_5m_input_tokens":0},
        "output_tokens":4},"total_cost_usd":0.425506}"#;

    #[test]
    fn a_ping_that_read_the_whole_prefix_is_warm() {
        // Real output from the 2026-09-03 probe.
        let o = parse_outcome(WARM).unwrap();
        assert_eq!(o.read, 54_882);
        assert_eq!(o.write, 0);
        assert!(matches!(effect(&o, 54_693), Effect::Warm { .. }));
    }

    #[test]
    fn a_ping_that_rewrote_the_prefix_reports_a_miss_not_a_saving() {
        let o = parse_outcome(COLD).unwrap();
        assert!(matches!(effect(&o, 54_693), Effect::Missed { .. }));
    }

    #[test]
    fn the_two_probes_reproduce_the_twenty_to_one_that_justifies_the_feature() {
        // Same session, same invocation, five minutes apart; the only variable
        // was whether the cache existed. If this ratio ever stops holding, the
        // feature has no reason to exist and should be deleted rather than tuned.
        let warm = parse_outcome(WARM).unwrap();
        let cold = parse_outcome(COLD).unwrap();
        let ratio = cold.cost_usd / warm.cost_usd;
        assert!(
            (ratio - 15.4).abs() < 0.5,
            "measured 15.4x on a 55k session, got {ratio:.1}x"
        );
        assert!((WRITE_MULT / READ_MULT - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_clean_hit_on_the_wrong_prefix_is_drift_not_success() {
        // The failure that looks exactly like success: write is zero, the ping
        // reports warm, and the pane still pays 2.0x when it comes back.
        let o = Outcome {
            read: 41_992,
            write: 0,
            out: 4,
            turns: 1,
            ..Outcome::default()
        };
        assert!(matches!(
            effect(&o, 54_693),
            Effect::Drifted {
                expected: 54_693,
                ..
            }
        ));
    }

    #[test]
    fn drift_inside_tolerance_is_still_warm() {
        // The ping appends its own prompt, so exact equality is not available.
        let o = Outcome {
            read: 54_882,
            write: 0,
            out: 4,
            turns: 1,
            ..Outcome::default()
        };
        assert!(matches!(effect(&o, 54_000), Effect::Warm { .. }));
    }

    #[test]
    fn a_short_ttl_ping_reports_itself_rather_than_looking_fine() {
        let o = Outcome {
            read: 0,
            write: 50_000,
            short_ttl: 50_000,
            turns: 1,
            ..Outcome::default()
        };
        assert_eq!(effect(&o, 54_693), Effect::ShortTtl);
    }

    #[test]
    fn a_noop_that_answered_at_length_is_reported() {
        let o = Outcome {
            read: 54_882,
            write: 0,
            out: 900,
            turns: 1,
            ..Outcome::default()
        };
        assert!(matches!(effect(&o, 54_693), Effect::Chatty { .. }));
    }

    #[test]
    fn a_failed_invocation_is_not_mistaken_for_a_cold_cache() {
        let o = Outcome {
            is_error: true,
            ..Outcome::default()
        };
        assert_eq!(effect(&o, 54_693), Effect::Failed);
    }

    #[test]
    fn a_warm_ping_reports_what_it_actually_bought() {
        let o = parse_outcome(WARM).unwrap();
        let Effect::Warm { saved_eq, .. } = effect(&o, 54_693) else {
            panic!("expected warm");
        };
        // 54,882 tokens that would have cost 2.0x now cost 0.1x, less the
        // 0.1x this ping spent: 54,882 * 1.9 - 5,488 ≈ 98,800.
        assert!((saved_eq - 98_787.6).abs() < 1.0, "got {saved_eq}");
        assert!(saved_eq > 0.0);
    }

    fn write_tmp(name: &str, body: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("td-ka-{}-{name}", std::process::id()));
        std::fs::write(&p, body).unwrap();
        p
    }

    const TURN: &str = r#"{"type":"assistant","timestamp":"2026-09-03T10:00:00.000Z","message":{"usage":{"cache_read_input_tokens":200000,"cache_creation_input_tokens":1200,"cache_creation":{"ephemeral_1h_input_tokens":1200,"ephemeral_5m_input_tokens":0}}}}"#;

    #[test]
    fn parked_state_comes_from_the_last_billed_turn() {
        let p = write_tmp("t1.jsonl", &format!("{TURN}\n"));
        let got = parked_from_transcript(&p, "s-1", "/home/parker").unwrap();
        std::fs::remove_file(&p).ok();
        assert_eq!(got.prefix, 201_200);
        assert_eq!(got.ttl, Ttl::Hour);
        assert_eq!(got.session, "s-1");
        assert_eq!(got.cwd, "/home/parker");
    }

    #[test]
    fn a_tool_result_moves_activity_without_moving_the_billed_turn() {
        // This is what keeps a pane mid-build from being pinged.
        let later = r#"{"type":"user","timestamp":"2026-09-03T10:40:00.000Z","message":{"role":"user","content":[{"type":"tool_result","content":"done"}]}}"#;
        let p = write_tmp("t2.jsonl", &format!("{TURN}\n{later}\n"));
        let got = parked_from_transcript(&p, "s-1", "/home/parker").unwrap();
        std::fs::remove_file(&p).ok();
        assert!(got.last_activity > got.last_billed);
        assert_eq!(got.last_activity - got.last_billed, 40 * 60);
    }

    #[test]
    fn a_five_minute_turn_is_detected_from_the_cache_creation_block() {
        let short = TURN.replace(
            r#""ephemeral_1h_input_tokens":1200,"ephemeral_5m_input_tokens":0"#,
            r#""ephemeral_1h_input_tokens":0,"ephemeral_5m_input_tokens":1200"#,
        );
        let p = write_tmp("t3.jsonl", &format!("{short}\n"));
        let got = parked_from_transcript(&p, "s-1", "/home/parker").unwrap();
        std::fs::remove_file(&p).ok();
        assert_eq!(got.ttl, Ttl::Short);
    }

    #[test]
    fn a_transcript_with_no_billed_turn_is_not_parked() {
        let p = write_tmp(
            "t4.jsonl",
            "{\"type\":\"user\",\"timestamp\":\"2026-09-03T10:00:00.000Z\"}\n",
        );
        let got = parked_from_transcript(&p, "s-1", "/home/parker");
        std::fs::remove_file(&p).ok();
        assert!(got.is_none());
    }

    #[test]
    fn unparseable_lines_are_skipped_not_fatal() {
        let p = write_tmp("t5.jsonl", &format!("not json\n{TURN}\n\n"));
        let got = parked_from_transcript(&p, "s-1", "/home/parker").unwrap();
        std::fs::remove_file(&p).ok();
        assert_eq!(got.prefix, 201_200);
    }

    #[test]
    fn a_missing_transcript_is_empty_not_an_error() {
        assert!(parked_from_transcript(
            Path::new("/nonexistent/td-keepalive.jsonl"),
            "s-1",
            "/home/parker"
        )
        .is_none());
    }

    #[test]
    fn the_cap_is_read_from_the_environment_but_only_within_reason() {
        // A cap of zero would ping forever-parked sessions; one of 500 hours
        // would too, slowly. Both fall back to the measured default.
        for bad in ["0", "500", "banana", ""] {
            std::env::set_var("TD_KEEPALIVE_CAP_HOURS", bad);
            assert_eq!(
                Config::from_env().cap.as_secs(),
                DEFAULT_CAP_HOURS * 3600,
                "{bad:?} should have fallen back"
            );
        }
        std::env::set_var("TD_KEEPALIVE_CAP_HOURS", "12");
        assert_eq!(Config::from_env().cap.as_secs(), 12 * 3600);
        std::env::remove_var("TD_KEEPALIVE_CAP_HOURS");
    }

    #[test]
    fn one_bridged_gap_pays_for_a_full_cap_of_pings() {
        // The arithmetic the whole feature stands on, at the average measured
        // gap: a 278k prefix (24.2M over 87 cold returns) parked for 5 hours.
        let prefix = 278_000.0;
        let pings = (5.0 * 3600.0 / PING_AFTER.as_secs() as f64).ceil();
        let ping_cost = pings * prefix * READ_MULT;
        let saved = prefix * (WRITE_MULT - READ_MULT);
        assert!(
            saved > ping_cost * 3.0,
            "saved {saved:.0} vs {pings} pings costing {ping_cost:.0}"
        );
    }

    #[test]
    fn the_cap_is_where_pinging_stops_paying_for_itself() {
        // Break-even is one bridged return against a cap's worth of pings:
        // 1.9x prefix saved / 0.1x prefix per ping ≈ 19 pings ≈ 17 hours. The
        // 8h default sits well inside it, which is the margin that absorbs
        // sessions that are never resumed.
        let per_hour = 3600.0 / PING_AFTER.as_secs() as f64;
        let breakeven_hours = (WRITE_MULT - READ_MULT) / READ_MULT / per_hour;
        assert!(breakeven_hours > 15.0 && breakeven_hours < 20.0);
        assert!((DEFAULT_CAP_HOURS as f64) < breakeven_hours / 2.0);
    }
}
