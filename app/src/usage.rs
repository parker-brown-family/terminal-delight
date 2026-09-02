//! Per-subscription agent usage — how much of each AI coding subscription is
//! spent, and how close it is to its ceiling.
//!
//! **TD collects nothing here.** Every number on this surface arrives as one
//! JSON record per subscription, written into a state directory by a *collector*
//! that knows how to talk to that vendor: Claude Code's transcripts plus
//! Anthropic's OAuth usage endpoint, Codex's app-server RPC, Fireworks' billing
//! API. This module reads the directory and hands the records to the panel.
//!
//! That split is the whole design, and it is why none of this requires Omarchy.
//! The record contract below is Omarchy's — its `omarchy-agent-usage-<agent>`
//! collectors publish it and its agents widget reads it — so on an Omarchy box
//! TD picks up records that are already being written and refreshed on somebody
//! else's timer, for free. On plain Ubuntu, Arch or Fedora nothing about reading
//! a directory of JSON changes; what is missing is only the writer, and a writer
//! is a plugin. Adding a subscription never touches this file either: publish a
//! record under a new `id` and the panel gains a tab.
//!
//! Two directories are searched, TD's own last, and a record present in both
//! resolves to whichever carries the newer `updatedAt`. So a machine that later
//! installs Omarchy does not end up with two stale halves of the same answer —
//! whoever collected most recently wins, regardless of who that was.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// One allowance and how much of it is gone.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Limit {
    /// The vendor's own words — "Session (5-hour)", "Weekly (7-day)".
    pub label: String,
    /// Fraction used, 0.0..=1.0 as published (NOT a percentage).
    pub percent: f32,
    /// RFC3339 instant the window rolls over, or empty if the vendor didn't say.
    pub resets_at: String,
}

/// Prepaid credit, for the subscriptions that bill from a balance instead of
/// resetting a window. `estimated` is the honest half: Fireworks' real ledger
/// endpoint is permission-gated, so the number is usually funding-minus-spend
/// rather than a figure the vendor stands behind.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Balance {
    pub remaining: f64,
    pub funded: f64,
    pub spent: f64,
    pub currency: String,
    pub estimated: bool,
}

/// One day's token total.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Day {
    pub date: String,
    pub tokens: u64,
}

/// One model's token split for the reporting window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelUse {
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_create: u64,
}

impl ModelUse {
    /// Every token the model was billed for, cache included — the same total the
    /// day rows carry, so a model row and a day row can be compared directly.
    pub fn total(&self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_create)
    }
}

/// One subscription's whole record.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record {
    pub id: String,
    pub name: String,
    /// The plan the subscription runs on — "Max 20x", "Pro", "Prepaid".
    pub tier_label: String,
    /// Set when the vendor could not be reached or the sign-in has expired; it
    /// replaces the plan line rather than sitting beside it.
    pub status_text: String,
    /// What to do about `status_text` ("Run `claude auth login` …").
    pub auth_help: String,
    /// The collector's own verdict on whether this record has anything to say.
    pub ready: bool,
    /// False for collectors that can only see tokens (a billing API reports no
    /// prompt or session counts), so today's line drops those two numbers rather
    /// than printing a confident zero.
    pub has_prompt_stats: bool,
    pub limits: Vec<Limit>,
    pub balance: Option<Balance>,
    pub today_prompts: u64,
    pub today_sessions: u64,
    pub today_tokens: u64,
    /// Last seven days, oldest first, today last.
    pub recent_days: Vec<Day>,
    /// Heaviest model first.
    pub models: Vec<ModelUse>,
    pub total_prompts: u64,
    pub total_sessions: u64,
    pub active_days: u64,
    /// RFC3339 instant the collector wrote this record.
    pub updated_at: String,
}

impl Record {
    /// Parse one record document. `id` is the only field a record cannot omit —
    /// it is the file's identity and the panel's tab key.
    pub fn from_json(v: &Value) -> Option<Self> {
        let s = |k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let u = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
        let b = |k: &str, dflt: bool| v.get(k).and_then(Value::as_bool).unwrap_or(dflt);
        let id = v.get("id").and_then(Value::as_str)?.to_string();
        if id.is_empty() {
            return None;
        }

        let limits = v
            .get("limits")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| {
                        let label = x
                            .get("label")
                            .or_else(|| x.get("title"))
                            .and_then(Value::as_str)?
                            .to_string();
                        Some(Limit {
                            label,
                            percent: x.get("percent").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                            resets_at: x
                                .get("resetsAt")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let balance = v.get("balance").and_then(Value::as_object).map(|o| {
            let f = |k: &str| o.get(k).and_then(Value::as_f64).unwrap_or(0.0);
            Balance {
                remaining: f("remaining"),
                funded: f("funded"),
                spent: f("spent"),
                currency: o
                    .get("currency")
                    .and_then(Value::as_str)
                    .unwrap_or("USD")
                    .to_string(),
                estimated: o.get("estimated").and_then(Value::as_bool).unwrap_or(true),
            }
        });

        // `messageCount` is the upstream key and it does NOT hold a message
        // count — every collector puts that day's TOKEN total in it, which is
        // why today's entry equals `todayTotalTokens` exactly. Named for what it
        // carries here, so nothing downstream has to remember the lie.
        let recent_days = v
            .get("recentDays")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|x| Day {
                        date: x
                            .get("date")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        tokens: x
                            .get("messageCount")
                            .or_else(|| x.get("tokens"))
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut models: Vec<ModelUse> = v
            .get("modelUsage")
            .and_then(Value::as_object)
            .map(|o| {
                o.iter()
                    .map(|(model, m)| {
                        let g = |k: &str| m.get(k).and_then(Value::as_u64).unwrap_or(0);
                        ModelUse {
                            model: model.clone(),
                            input: g("inputTokens"),
                            output: g("outputTokens"),
                            cache_read: g("cacheReadInputTokens"),
                            cache_create: g("cacheCreationInputTokens"),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        models.sort_by(|a, b| b.total().cmp(&a.total()).then(a.model.cmp(&b.model)));

        Some(Record {
            name: {
                let n = s("name");
                if n.is_empty() {
                    id.clone()
                } else {
                    n
                }
            },
            id,
            tier_label: s("tierLabel"),
            status_text: s("usageStatusText"),
            auth_help: s("authHelpText"),
            ready: b("ready", false),
            has_prompt_stats: b("hasPromptStats", true),
            limits,
            balance,
            today_prompts: u("todayPrompts"),
            today_sessions: u("todaySessions"),
            today_tokens: u("todayTotalTokens"),
            recent_days,
            models,
            total_prompts: u("totalPrompts"),
            total_sessions: u("totalSessions"),
            active_days: u("activeDays"),
            updated_at: s("updatedAt"),
        })
    }

    /// Every token in the reporting window, across models.
    pub fn total_tokens(&self) -> u64 {
        self.models
            .iter()
            .fold(0u64, |acc, m| acc.saturating_add(m.total()))
    }

    /// The limit under the most pressure, which is the one worth showing when
    /// there is only room for one number.
    pub fn worst_limit(&self) -> Option<&Limit> {
        self.limits
            .iter()
            .max_by(|a, b| a.percent.total_cmp(&b.percent))
    }

    /// Does this record have anything to draw? `ready` is the collector's own
    /// verdict, but a record carrying limits or tokens is worth showing whatever
    /// it claims — the flag goes false on a transport failure that left real
    /// local stats intact.
    pub fn has_content(&self) -> bool {
        self.ready || !self.limits.is_empty() || self.total_tokens() > 0
    }
}

/// `$XDG_STATE_HOME`, or the spec's fallback.
fn state_home(home: &Path) -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/state"))
}

/// Where records are looked for, in order. TD's own directory comes last so that
/// a record it wrote itself never shadows a fresher one somebody else collected;
/// [`read_all`] resolves the overlap by `updatedAt`, not by position.
pub fn dirs(home: &Path) -> Vec<PathBuf> {
    let state = state_home(home);
    vec![
        state.join("omarchy/agents/usage"),
        state.join("terminal-delight/agents/usage"),
    ]
}

/// Read every record on disk, newest-wins per `id`, heaviest subscription first.
///
/// Never fails: a missing directory, an unreadable file and a truncated record
/// are all simply absent from the result. There is no error state to render,
/// because "nothing collected anything yet" and "the collector is broken" look
/// identical from here and the panel says so in one line either way.
pub fn read_all(home: &Path) -> Vec<Record> {
    let mut out: Vec<Record> = Vec::new();
    for dir in dirs(home) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let Ok(txt) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&txt) else {
                continue;
            };
            let Some(rec) = Record::from_json(&v) else {
                continue;
            };
            match out.iter_mut().find(|r| r.id == rec.id) {
                // String compare is right for RFC3339 in the same offset, and
                // every collector stamps UTC. A tie keeps what we already had.
                Some(prev) if rec.updated_at > prev.updated_at => *prev = rec,
                Some(_) => {}
                None => out.push(rec),
            }
        }
    }
    out.sort_by(|a, b| {
        b.has_content()
            .cmp(&a.has_content())
            .then(b.total_tokens().cmp(&a.total_tokens()))
            .then(a.name.cmp(&b.name))
    });
    out
}

/// The command that regenerates every record, if this machine has one.
///
/// Omarchy's updater is preferred when present because it is the one already
/// wired to that box's collectors and settings; TD's own plugin binary is the
/// fallback for every other Linux. Returning `None` is not an error — it means
/// the panel draws what is on disk and names the writer it is missing.
pub fn refresh_command(home: &Path) -> Option<(String, Vec<String>)> {
    if let Some(p) = which("omarchy-agent-usage-update") {
        return Some((p, vec![]));
    }
    if let Some(p) = which("td-agent-usage") {
        return Some((p, vec!["update".into()]));
    }
    let local = home.join(".local/bin/td-agent-usage");
    if local.is_file() {
        return Some((local.to_string_lossy().into_owned(), vec!["update".into()]));
    }
    None
}

/// Run the updater and wait for it. Slow on purpose — it is talking to vendor
/// endpoints — so this belongs on a background thread, never on the frame.
pub fn refresh(home: &Path) -> Result<(), String> {
    let Some((cmd, args)) = refresh_command(home) else {
        return Err("no usage collector installed".into());
    };
    let out = std::process::Command::new(&cmd)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("could not run {cmd}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().next().unwrap_or("collector failed").trim();
        Err(line.to_string())
    }
}

fn which(bin: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(bin))
        .find(|c| c.is_file())
        .map(|c| c.to_string_lossy().into_owned())
}

// ----------------------------------------------------------------------------
// time, in the two shapes this panel needs
// ----------------------------------------------------------------------------

/// Seconds since the epoch for an RFC3339 instant, or `None` if it isn't one.
///
/// Written out rather than pulled in: TD's whole dependency list is four crates
/// and a patched gpui, and this needs one date shape from one trusted writer —
/// `2026-09-02T19:59:59.724228+00:00`, `…Z`, or a bare local-looking stamp.
/// Fractional seconds are skipped, not rounded; a reset countdown does not care.
pub fn parse_epoch(ts: &str) -> Option<i64> {
    let b = ts.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let num = |a: usize, z: usize| ts.get(a..z)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, s) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    let mut secs = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s;
    // Trailing zone: Z, ±HH:MM or ±HHMM. Anything else is read as UTC, which is
    // what every collector writes anyway.
    let rest = &ts[19..];
    let zone = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    if let Some(sign) = zone.chars().next() {
        if sign == '+' || sign == '-' {
            let z = &zone[1..].replace(':', "");
            if z.len() >= 4 {
                let oh: i64 = z[0..2].parse().ok()?;
                let om: i64 = z[2..4].parse().ok()?;
                let off = oh * 3600 + om * 60;
                secs += if sign == '-' { off } else { -off };
            }
        }
    }
    Some(secs)
}

/// Days from 1970-01-01 to y-m-d, proleptic Gregorian. Hinnant's `days_from_civil`.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Now, in seconds since the epoch.
pub fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// "3h 47m", "12m", "6d 4h" — how long until `ts`. `None` when the stamp is
/// unparseable or already past, so a stale window renders as nothing rather than
/// as a confident negative.
pub fn until(ts: &str, now: i64) -> Option<String> {
    let then = parse_epoch(ts)?;
    let left = then - now;
    if left <= 0 {
        return None;
    }
    Some(compact_duration(left))
}

/// "just now", "4m ago", "3h ago" — how long since `ts`.
pub fn since(ts: &str, now: i64) -> Option<String> {
    let then = parse_epoch(ts)?;
    let ago = now - then;
    if ago < 45 {
        return Some("just now".into());
    }
    Some(format!("{} ago", compact_duration(ago)))
}

/// "Mon", "Tue", … for a bare `YYYY-MM-DD`, empty when it isn't one. The epoch
/// began on a Thursday, which is why the table starts there.
pub fn weekday(date: &str) -> &'static str {
    const NAMES: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    match parse_epoch(&format!("{date}T00:00:00Z")) {
        Some(e) => NAMES[(((e / 86_400) % 7 + 7) % 7) as usize],
        None => "",
    }
}

fn compact_duration(secs: i64) -> String {
    let (d, h, m) = (secs / 86_400, (secs % 86_400) / 3600, (secs % 3600) / 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{}m", m.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE: &str = r#"{
      "schemaVersion":1,"id":"claude","name":"Claude Code","ready":true,
      "tierLabel":"Max 20x","usageStatusText":"","authHelpText":"login",
      "updatedAt":"2026-09-02T16:12:19.808926+00:00",
      "limits":[{"label":"Session (5-hour)","percent":0.22,"resetsAt":"2026-09-02T19:59:59+00:00"},
                {"label":"Weekly (7-day)","percent":0.32,"resetsAt":"2026-09-07T16:59:59+00:00"}],
      "todayPrompts":2568,"todaySessions":20,"todayTotalTokens":680266339,
      "recentDays":[{"date":"2026-09-01","messageCount":757994890},
                    {"date":"2026-09-02","messageCount":680266339}],
      "modelUsage":{"claude-sonnet-5":{"inputTokens":384,"outputTokens":1014,
                                        "cacheReadInputTokens":24917886,"cacheCreationInputTokens":546708},
                    "claude-opus-5":{"inputTokens":12978,"outputTokens":5667499,
                                      "cacheReadInputTokens":1407140196,"cacheCreationInputTokens":34309740}},
      "totalPrompts":8241,"totalSessions":64,"activeDays":4
    }"#;

    fn parse(s: &str) -> Record {
        Record::from_json(&serde_json::from_str(s).unwrap()).unwrap()
    }

    #[test]
    fn parses_a_real_claude_record() {
        let r = parse(CLAUDE);
        assert_eq!(r.id, "claude");
        assert_eq!(r.name, "Claude Code");
        assert_eq!(r.tier_label, "Max 20x");
        assert!(r.ready);
        assert_eq!(r.limits.len(), 2);
        assert_eq!(r.today_tokens, 680_266_339);
        // hasPromptStats is absent here and must default TRUE — only a collector
        // that cannot see prompts says so, and it says so explicitly.
        assert!(r.has_prompt_stats);
    }

    #[test]
    fn day_rows_carry_tokens_despite_the_upstream_key() {
        let r = parse(CLAUDE);
        let today = r.recent_days.last().unwrap();
        assert_eq!(today.date, "2026-09-02");
        assert_eq!(today.tokens, r.today_tokens);
    }

    #[test]
    fn models_sort_heaviest_first_and_total_includes_cache() {
        let r = parse(CLAUDE);
        assert_eq!(r.models[0].model, "claude-opus-5");
        assert!(r.models[0].total() > r.models[1].total());
        assert_eq!(r.models[1].total(), 384 + 1014 + 24_917_886 + 546_708);
    }

    #[test]
    fn worst_limit_is_the_one_under_pressure() {
        let r = parse(CLAUDE);
        assert_eq!(r.worst_limit().unwrap().label, "Weekly (7-day)");
    }

    #[test]
    fn a_record_without_an_id_is_not_a_record() {
        assert!(Record::from_json(&serde_json::json!({"name": "nameless"})).is_none());
        assert!(Record::from_json(&serde_json::json!({"id": ""})).is_none());
    }

    #[test]
    fn a_limit_may_name_itself_with_title() {
        let r = parse(r#"{"id":"x","limits":[{"title":"Fable Weekly","percent":0.34}]}"#);
        assert_eq!(r.limits[0].label, "Fable Weekly");
    }

    #[test]
    fn prepaid_balance_parses_and_defaults_to_estimated() {
        let r = parse(r#"{"id":"fireworks","balance":{"remaining":12.5,"funded":20,"spent":7.5}}"#);
        let b = r.balance.unwrap();
        assert_eq!(b.remaining, 12.5);
        assert_eq!(b.currency, "USD");
        assert!(b.estimated);
    }

    #[test]
    fn an_unready_record_with_numbers_still_has_content() {
        // A transport failure sets ready=false while local stats survive; hiding
        // that record would throw away the only numbers the user has.
        let r = parse(
            r#"{"id":"codex","ready":false,
                "modelUsage":{"gpt-5":{"inputTokens":10,"outputTokens":5}}}"#,
        );
        assert!(!r.ready);
        assert!(r.has_content());
        let empty = parse(r#"{"id":"nothing","ready":false}"#);
        assert!(!empty.has_content());
    }

    #[test]
    fn epoch_round_trips_the_shapes_collectors_write() {
        assert_eq!(parse_epoch("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_epoch("2026-09-02T19:59:59.724228+00:00"),
            Some(1_788_379_199)
        );
    }

    #[test]
    fn epoch_applies_the_offset_in_the_right_direction() {
        let utc = parse_epoch("2026-09-02T12:00:00Z").unwrap();
        // -07:00 is seven hours BEHIND UTC, so the same wall clock is later.
        assert_eq!(
            parse_epoch("2026-09-02T12:00:00-07:00").unwrap(),
            utc + 25_200
        );
        assert_eq!(
            parse_epoch("2026-09-02T12:00:00+02:00").unwrap(),
            utc - 7_200
        );
        assert_eq!(
            parse_epoch("2026-09-02T12:00:00+0200").unwrap(),
            utc - 7_200
        );
    }

    #[test]
    fn garbage_is_not_a_timestamp() {
        assert!(parse_epoch("").is_none());
        assert!(parse_epoch("soon").is_none());
        assert!(parse_epoch("2026-13-02T12:00:00Z").is_none());
    }

    #[test]
    fn weekdays_land_on_the_right_day() {
        // 1970-01-01 was a Thursday; 2026-09-02 is a Wednesday.
        assert_eq!(weekday("1970-01-01"), "Thu");
        assert_eq!(weekday("2026-09-02"), "Wed");
        assert_eq!(weekday("2026-09-06"), "Sun");
        assert_eq!(weekday("not-a-date"), "");
    }

    #[test]
    fn countdowns_read_the_way_a_person_says_them() {
        let now = parse_epoch("2026-09-02T12:00:00Z").unwrap();
        assert_eq!(
            until("2026-09-02T15:47:00Z", now).as_deref(),
            Some("3h 47m")
        );
        assert_eq!(until("2026-09-02T12:12:00Z", now).as_deref(), Some("12m"));
        assert_eq!(until("2026-09-08T16:00:00Z", now).as_deref(), Some("6d 4h"));
        // a window that has already rolled over renders as nothing, never as -3h
        assert_eq!(until("2026-09-02T11:00:00Z", now), None);
        assert_eq!(
            since("2026-09-02T11:56:00Z", now).as_deref(),
            Some("4m ago")
        );
        assert_eq!(
            since("2026-09-02T11:59:50Z", now).as_deref(),
            Some("just now")
        );
    }
}
