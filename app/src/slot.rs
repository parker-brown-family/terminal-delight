//! The left bar's bottom slot — the widgets that live under the project tree.
//!
//! The empty space beneath the tree becomes a stack of small widgets: an
//! allowance row per AI subscription, and the agent-state rollup shrunk to one
//! line. This module is the slot's arithmetic and nothing else — no gpui, so
//! every rule below is a unit test rather than a screenshot. `main.rs` paints
//! what these functions decide.
//!
//! **The rule this module exists to hold.** A subscription with no collector
//! installed, an expired sign-in, or a vendor that publishes no weekly window
//! produces *no reading*, and a bar's natural default for no reading is a bar of
//! length zero — which reads as "you are out". That is the exact inversion of
//! the truth, drawn confidently, in the one place a glance is all anyone gives
//! it. So absence is a variant of [`Reading`] and never a number, the hatch is
//! decided before the colour ramp is ever consulted, and there is a test per
//! state saying so.

/// One allowance window's reading, with absence modelled rather than defaulted.
///
/// The payload is the fraction **spent**, because that is what the vendor
/// publishes (`usage::Limit::percent`) — remaining is computed at paint and
/// never stored, so a display toggle can never rewrite what a record says.
#[derive(Clone, Debug, PartialEq)]
pub enum Reading {
    /// The vendor published a number and the record is current.
    Fresh(f32),
    /// The number was true and is no longer current. It dims and carries its
    /// age rather than disappearing: a four-hour-old session reading is still
    /// worth more than nothing.
    Stale { spent: f32, age: String },
    /// No collector, an expired sign-in, or a vendor with no such window. Not a
    /// zero, not an error, and not something to draw a bar for.
    Unknown,
}

impl Reading {
    /// The fraction spent, or `None` when there is no reading. Deliberately an
    /// `Option` all the way to the paint site: the collapse to a drawable
    /// number happens in the renderer, where a person can see the hatch and
    /// argue with it, and nowhere earlier.
    pub fn spent(&self) -> Option<f32> {
        match self {
            Reading::Fresh(p) | Reading::Stale { spent: p, .. } => Some(p.clamp(0., 1.)),
            Reading::Unknown => None,
        }
    }

    /// What is left, computed here and never stored. `None` when unknown.
    pub fn remaining(&self) -> Option<f32> {
        self.spent().map(|p| 1.0 - p)
    }

    /// True only for the absent case. The renderer asks this *first* — a ramp
    /// reaching for an intensity needs a number to compute it from, and a
    /// missing window has none.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Reading::Unknown)
    }

    /// True when the number is real but no longer current.
    pub fn is_stale(&self) -> bool {
        matches!(self, Reading::Stale { .. })
    }

    /// The age string a stale reading carries, if any.
    pub fn age(&self) -> Option<&str> {
        match self {
            Reading::Stale { age, .. } => Some(age.as_str()),
            _ => None,
        }
    }
}

/// Which band of the house pressure ramp a reading falls in.
///
/// Hue is *not* themed and not this module's to choose — `main.rs` owns the
/// three colours, and has since the usage panel shipped. This enum only says
/// which of them applies, so the rule is testable without a renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    /// Under two thirds spent.
    Calm,
    /// Past two thirds.
    Warn,
    /// The last sixth.
    Spent,
}

/// The band for a spent fraction — green under two thirds, amber past it, red
/// in the last sixth. The same thresholds the usage panel has always used.
pub fn band(spent: f32) -> Band {
    if spent >= 0.85 {
        Band::Spent
    } else if spent >= 0.66 {
        Band::Warn
    } else {
        Band::Calm
    }
}

/// The five sample points of the urgency ramp as it was drawn, keyed on spent.
///
/// Hue says *which* colour; this says *how loud*. A bar with room is dim and a
/// bar nearly out is at full strength, so the scarce thing is the only bright
/// object in the column even when two rows share a colour.
const RAMP: [(f32, f32); 5] = [
    (0.00, 0.32),
    (0.30, 0.45),
    (0.60, 0.72),
    (0.85, 0.93),
    (0.96, 1.00),
];

/// How strongly a bar paints, for a fraction spent. Piecewise-linear through
/// the ramp as drawn, so the shipped widget matches the figure that was
/// approved at its own sample points rather than approximately.
pub fn intensity(spent: f32) -> f32 {
    let s = spent.clamp(0., 1.);
    if s <= RAMP[0].0 {
        return RAMP[0].1;
    }
    for w in RAMP.windows(2) {
        let (x0, y0) = w[0];
        let (x1, y1) = w[1];
        if s <= x1 {
            let t = (s - x0) / (x1 - x0);
            return y0 + t * (y1 - y0);
        }
    }
    RAMP[RAMP.len() - 1].1
}

/// How many ticks a session rail carries, read out of the vendor's own label.
///
/// The window names its unit and the tick is that unit: "Session (5-hour)" is
/// five ticks because a tick is an hour, "Weekly (7-day)" is seven because a
/// tick is a day. A label that names no unit we can read falls back to six —
/// a count chosen to look like a count and not to imply an hour or a day.
pub fn tick_count(label: &str) -> u8 {
    const UNITS: [&str; 8] = ["hour", "hr", "day", "week", "wk", "minute", "min", "month"];
    let low = label.to_ascii_lowercase();
    let bytes = low.as_bytes();
    for unit in UNITS {
        let mut from = 0usize;
        while let Some(rel) = low[from..].find(unit) {
            let at = from + rel;
            // Walk back over one separator and then the digits touching it, so
            // "5-hour", "5 hour" and "5hour" all read as five.
            let mut i = at;
            while i > 0 && matches!(bytes[i - 1], b' ' | b'-' | b'_') {
                i -= 1;
            }
            let end = i;
            while i > 0 && bytes[i - 1].is_ascii_digit() {
                i -= 1;
            }
            if i < end {
                if let Ok(n) = low[i..end].parse::<u32>() {
                    if (2..=12).contains(&n) {
                        return n as u8;
                    }
                }
            }
            from = at + unit.len();
        }
    }
    6
}

/// How many of `n` ticks are lit for a remaining fraction.
///
/// Rounds to nearest so a full window shows a full rail, and a window with any
/// meaningful room left never shows an empty one: a non-zero remainder lights
/// at least one tick, for the same reason the usage panel's meter floors a
/// non-zero share at two percent of its track.
pub fn ticks_lit(remaining: f32, n: u8) -> u8 {
    if n == 0 {
        return 0;
    }
    let r = remaining.clamp(0., 1.);
    let lit = (r * f32::from(n)).round() as u8;
    if lit == 0 && r > 0. {
        1
    } else {
        lit.min(n)
    }
}

/// What the widget drops as the bar narrows, and in which order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Degrade {
    /// The trailing percentage. First to go — it is a convenience, and the bar
    /// itself is the reading.
    pub show_pct: bool,
    /// Ticks actually drawn, quantised down from what the label asked for. A
    /// tick narrower than two pixels is a texture, not a count.
    pub ticks: u8,
}

/// The bar width at which the trailing percentage stops earning its cell.
const PCT_FLOOR: f32 = 190.;
/// Width a tick needs — two pixels of tick and one of gutter.
const TICK_MIN: f32 = 3.;

impl Degrade {
    /// Resolve the degradation for a bar of `bar_w` logical pixels (before
    /// scale) asking for `want` ticks, given the `rail_w` the rail itself gets.
    ///
    /// The glyphs are not in this list: losing them makes the bar anonymous,
    /// and an anonymous bar in a sidebar is worse than no bar.
    pub fn resolve(bar_w: f32, rail_w: f32, want: u8) -> Self {
        let show_pct = bar_w >= PCT_FLOOR;
        let mut ticks = want.max(3);
        for candidate in [want, 5, 3] {
            if candidate <= want && rail_w / f32::from(candidate.max(1)) >= TICK_MIN {
                ticks = candidate;
                break;
            }
            ticks = 3;
        }
        Degrade { show_pct, ticks }
    }
}

/// One subscription's row: the mark, and the two windows it publishes.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderRow {
    /// The record's own id, which is the row's identity and the tab key.
    pub id: String,
    /// What the vendor calls itself, for the hover and the menu.
    pub name: String,
    /// Up to two characters standing in for the vendor's mark.
    ///
    /// A real brand asset under its brand guidelines is the normal answer and
    /// is a licensing question rather than a drawing one; a glyph published by
    /// the collector is the answer for a provider TD has never heard of. This
    /// is the third way out, and it has to exist whatever happens to the other
    /// two, because the provider list is open-ended by construction.
    pub initials: String,
    /// The long window — drawn as the solid rail.
    pub week: Reading,
    /// The short window — drawn as the notched rail.
    pub session: Reading,
    /// Ticks the session rail asks for, from its own label.
    pub session_ticks: u8,
    /// The vendor's words for the session window, for the hover.
    pub session_label: String,
    /// The vendor's words for the long window, for the hover.
    pub week_label: String,
}

/// How many hours the window named by a label spans, when it names one.
///
/// Used for one decision only: whether a record is older than the window it
/// describes, which is what makes a number stale rather than merely old. A
/// weekly figure collected two hours ago is current; a five-hour session figure
/// collected six hours ago describes a window that has since rolled over.
pub fn window_hours(label: &str) -> Option<f32> {
    let low = label.to_ascii_lowercase();
    let bytes = low.as_bytes();
    for (unit, hours) in [
        ("hour", 1.0f32),
        ("hr", 1.0),
        ("day", 24.0),
        ("week", 168.0),
        ("wk", 168.0),
        ("month", 720.0),
        ("minute", 1.0 / 60.0),
    ] {
        let mut from = 0usize;
        while let Some(rel) = low[from..].find(unit) {
            let at = from + rel;
            let mut i = at;
            while i > 0 && matches!(bytes[i - 1], b' ' | b'-' | b'_') {
                i -= 1;
            }
            let end = i;
            while i > 0 && bytes[i - 1].is_ascii_digit() {
                i -= 1;
            }
            if i < end {
                if let Ok(n) = low[i..end].parse::<u32>() {
                    return Some(n as f32 * hours);
                }
            }
            // "Weekly" with no count still names a span.
            if unit == "week" || unit == "wk" {
                return Some(168.0);
            }
            from = at + unit.len();
        }
    }
    None
}

impl ProviderRow {
    /// Read one subscription's row out of the record the collector published.
    ///
    /// `age_hours` is how long ago the record was written, or `None` when the
    /// record does not say — which is itself an unknown and is treated as one
    /// rather than as "just now".
    ///
    /// The order here is the contract. Absence is decided **first**, from the
    /// collector's own verdict and from whether the window exists at all; only
    /// a reading that survives that gets a number, and only a number gets a
    /// colour. Nothing downstream can turn an `Unknown` back into a zero.
    pub fn from_record(rec: &crate::usage::Record, age_hours: Option<f32>) -> Self {
        // The collector's verdict outranks anything in the payload: a record
        // that says the sign-in expired has stale numbers at best, and drawing
        // them as current would be the confident-wrong-number failure.
        let mute = !rec.ready || !rec.status_text.is_empty();

        let pick = |want_session: bool| -> (Reading, String) {
            if mute {
                return (Reading::Unknown, String::new());
            }
            let found = rec.limits.iter().find(|l| {
                if want_session {
                    looks_like_session(&l.label)
                } else {
                    looks_like_week(&l.label)
                }
            });
            // Fall back to position only when the labels say nothing: first is
            // the short window, last is the long one. A single unlabelled limit
            // is the session, and the week stays unknown rather than borrowing
            // the session's number.
            let found = found.or_else(|| {
                if rec
                    .limits
                    .iter()
                    .any(|l| looks_like_session(&l.label) || looks_like_week(&l.label))
                {
                    return None;
                }
                if want_session {
                    rec.limits.first()
                } else {
                    rec.limits.get(1)
                }
            });
            let Some(l) = found else {
                return (Reading::Unknown, String::new());
            };
            let span = window_hours(&l.label);
            let reading = match (age_hours, span) {
                // Older than the window it describes: the number was true and
                // is no longer current.
                (Some(age), Some(h)) if age > h => Reading::Stale {
                    spent: l.percent,
                    age: round_age(age),
                },
                (Some(_), _) => Reading::Fresh(l.percent),
                // The record does not say when it was written. That is not
                // "now"; it is one more thing nobody has measured.
                (None, _) => Reading::Stale {
                    spent: l.percent,
                    age: "?".into(),
                },
            };
            (reading, l.label.clone())
        };

        let (session, session_label) = pick(true);
        let (week, week_label) = pick(false);
        ProviderRow {
            id: rec.id.clone(),
            name: rec.name.clone(),
            initials: initials(&rec.name, &rec.id),
            session_ticks: tick_count(&session_label),
            week,
            session,
            session_label,
            week_label,
        }
    }
}

/// A coarse age for a stale badge — hours below a day, then days.
fn round_age(hours: f32) -> String {
    if hours < 1.0 {
        format!("{}m", (hours * 60.).round().max(1.) as i64)
    } else if hours < 24.0 {
        format!("{}h", hours.round() as i64)
    } else {
        format!("{}d", (hours / 24.).round() as i64)
    }
}

/// Two initials for a provider that ships no mark — first letters of the first
/// two words, or the first two letters of a single word.
pub fn initials(name: &str, id: &str) -> String {
    let src = if name.trim().is_empty() { id } else { name };
    let words: Vec<&str> = src
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let out: String = match words.len() {
        0 => String::new(),
        1 => words[0].chars().take(2).collect(),
        _ => words
            .iter()
            .take(2)
            .filter_map(|w| w.chars().next())
            .collect(),
    };
    out.to_uppercase()
}

/// True when a label names the short, rolling window rather than the long one.
fn looks_like_session(label: &str) -> bool {
    let low = label.to_ascii_lowercase();
    low.contains("session")
        || low.contains("hour")
        || low.contains("hr")
        || low.contains("5h")
        || low.contains("current")
}

/// True when a label names the long window.
fn looks_like_week(label: &str) -> bool {
    let low = label.to_ascii_lowercase();
    low.contains("week") || low.contains("wk") || low.contains("day") || low.contains("month")
}

/// The counts behind the one-line agent rollup.
///
/// The wall already tallies exactly this; the widget is the same tally at one
/// line. Zero is a real answer here and is drawn dim — unlike an allowance, a
/// count of zero agents in a state is a measurement, not an absence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub working: u32,
    pub blocked: u32,
    pub errored: u32,
    pub finished: u32,
    pub idle: u32,
}

impl Tally {
    /// Every agent pane counted.
    pub fn total(&self) -> u32 {
        self.working + self.blocked + self.errored + self.finished + self.idle
    }

    /// True when something in the tally wants a human — the whole point of the
    /// rollup being in a sidebar that is on screen constantly.
    pub fn needs_you(&self) -> bool {
        self.blocked > 0 || self.errored > 0
    }
}

// ---- the provider's own mark, if the user has one -------------------------

/// Where a mark for provider `id` would be: `<config>/marks/<id>.svg`.
///
/// **Why a directory the user fills rather than files this repo ships.** A
/// vendor's logo is that vendor's trademark, and TD is public and MIT — so the
/// marks cannot live in the tree, however much better they look than initials.
/// They also cannot be drawn: the OpenAI blossom is an interlocking knot, and a
/// hand-approximated version of somebody's logo is worse than no logo, because
/// it is wrong in a way that looks deliberate.
///
/// A directory solves all of it at once. Whoever runs TD drops the mark they
/// are entitled to use, under the id the collector already publishes, and
/// [`ProviderRow::initials`] stays as the fallback that ships — which it had to
/// be anyway, since the provider list is open-ended by construction.
///
/// One path segment only: an id is a filename here, so anything that could
/// climb out of the directory disqualifies it. Rendered by gpui as a
/// single-colour mask, so a multi-colour file is drawn in one tint and a file
/// that is not an SVG at all is simply not drawn.
pub fn mark_path(config: &std::path::Path, id: &str) -> Option<std::path::PathBuf> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    let path = config.join("marks").join(format!("{id}.svg"));
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- the provider mark directory ------------------------------------

    #[test]
    fn a_mark_is_found_by_id_and_only_inside_its_own_directory() {
        let tmp = std::env::temp_dir().join(format!("td-marks-{}", std::process::id()));
        let marks = tmp.join("marks");
        std::fs::create_dir_all(&marks).unwrap();
        std::fs::write(marks.join("claude.svg"), "<svg/>").unwrap();
        std::fs::write(tmp.join("outside.svg"), "<svg/>").unwrap();

        assert_eq!(mark_path(&tmp, "claude"), Some(marks.join("claude.svg")));
        // a provider with no mark falls back to initials, which is the whole
        // reason initials are not optional
        assert_eq!(mark_path(&tmp, "codex"), None);

        // An id is a FILENAME here. Every one of these reaches a real file if
        // the id is pasted into the path unchecked, and the first two reach one
        // that exists in this fixture.
        for hostile in [
            "../outside",
            "..",
            "marks/../../outside",
            "/etc/hostname",
            "a b",
            "",
        ] {
            assert_eq!(
                mark_path(&tmp, hostile),
                None,
                "{hostile:?} must not resolve to a mark"
            );
        }
        std::fs::remove_dir_all(&tmp).ok();
    }

    // ---- the rule the whole module exists for ---------------------------

    #[test]
    fn an_absent_allowance_is_not_a_spent_one() {
        // The failure this guards is a one-character default: `unwrap_or(0.0)`
        // on a missing percent draws a full-red empty bar and tells you that a
        // subscription you have never collected is exhausted.
        assert_eq!(Reading::Unknown.spent(), None);
        assert_eq!(Reading::Unknown.remaining(), None);
        assert!(Reading::Unknown.is_unknown());

        // And the honest zero is still available, distinctly.
        assert_eq!(Reading::Fresh(0.0).spent(), Some(0.0));
        assert!(!Reading::Fresh(0.0).is_unknown());
        assert_ne!(Reading::Fresh(0.0), Reading::Unknown);
    }

    #[test]
    fn a_stale_reading_keeps_its_number_and_its_age() {
        let r = Reading::Stale {
            spent: 0.4,
            age: "4h".into(),
        };
        assert_eq!(r.spent(), Some(0.4));
        assert_eq!(r.age(), Some("4h"));
        assert!(r.is_stale());
        // Stale is a third thing: it is not absent, so it still draws a bar.
        assert!(!r.is_unknown());
    }

    #[test]
    fn remaining_is_computed_and_never_stored() {
        assert_eq!(Reading::Fresh(0.87).remaining().unwrap(), 0.13);
        // The payload is untouched by having been read as remaining.
        let r = Reading::Fresh(0.87);
        let _ = r.remaining();
        assert_eq!(r.spent(), Some(0.87));
    }

    // ---- the tick count, read out of the vendor's own words -------------

    #[test]
    fn the_window_names_its_own_tick() {
        assert_eq!(tick_count("Session (5-hour)"), 5);
        assert_eq!(tick_count("Weekly (7-day)"), 7);
        assert_eq!(tick_count("5 hour session"), 5);
        assert_eq!(tick_count("7day"), 7);
        assert_eq!(tick_count("Monthly (30-day)"), 6, "30 is not a tick count");
    }

    #[test]
    fn a_label_that_names_no_unit_falls_back_to_six() {
        assert_eq!(tick_count("Weekly limit"), 6);
        assert_eq!(tick_count(""), 6);
        assert_eq!(tick_count("Prepaid balance"), 6);
    }

    // ---- the ramp ------------------------------------------------------

    #[test]
    fn hue_is_the_house_ramp_and_nothing_new() {
        assert_eq!(band(0.10), Band::Calm);
        assert_eq!(band(0.65), Band::Calm);
        assert_eq!(band(0.66), Band::Warn);
        assert_eq!(band(0.84), Band::Warn);
        assert_eq!(band(0.85), Band::Spent);
        assert_eq!(band(1.00), Band::Spent);
    }

    #[test]
    fn intensity_matches_the_ramp_that_was_drawn() {
        // The five rows of the approved figure, keyed on spent.
        for (spent, want) in RAMP {
            let got = intensity(spent);
            assert!(
                (got - want).abs() < 0.001,
                "spent {spent} wanted {want} got {got}"
            );
        }
    }

    #[test]
    fn a_bar_with_room_is_dimmer_than_a_bar_nearly_out() {
        assert!(intensity(0.09) < intensity(0.87));
        // Monotonic across the whole domain, so two rows never mislead by
        // relative brightness.
        let mut last = -1.0f32;
        for i in 0..=100 {
            let v = intensity(i as f32 / 100.);
            assert!(v >= last, "not monotonic at {i}");
            last = v;
        }
    }

    // ---- the rails -----------------------------------------------------

    #[test]
    fn the_decided_figures_numbers_land_as_drawn() {
        // The approved mock: session 9% spent, week 87% spent.
        let session = Reading::Fresh(0.09);
        let week = Reading::Fresh(0.87);
        // Week reads 13% remaining, in red, at full strength.
        assert_eq!((week.remaining().unwrap() * 100.).round() as i32, 13);
        assert_eq!(band(0.87), Band::Spent);
        assert!(intensity(0.87) > 0.9);
        // Session lights all five of its ticks, and sits dim.
        assert_eq!(ticks_lit(session.remaining().unwrap(), 5), 5);
        assert!(intensity(0.09) < 0.45);
    }

    #[test]
    fn a_window_with_room_left_never_shows_an_empty_rail() {
        assert_eq!(ticks_lit(0.01, 5), 1);
        assert_eq!(ticks_lit(0.0, 5), 0, "actually empty still reads empty");
        assert_eq!(ticks_lit(1.0, 5), 5);
        assert_eq!(ticks_lit(0.6, 5), 3);
    }

    // ---- narrowing -----------------------------------------------------

    #[test]
    fn the_percentage_goes_before_the_ticks_do() {
        let wide = Degrade::resolve(260., 150., 7);
        assert!(wide.show_pct);
        assert_eq!(wide.ticks, 7);

        let narrow = Degrade::resolve(150., 150., 7);
        assert!(!narrow.show_pct, "the convenience goes first");
        assert_eq!(narrow.ticks, 7, "the reading is still intact");
    }

    #[test]
    fn ticks_quantise_down_rather_than_drawing_a_grey_mush() {
        assert_eq!(Degrade::resolve(260., 150., 7).ticks, 7);
        assert_eq!(Degrade::resolve(260., 18., 7).ticks, 5);
        assert_eq!(Degrade::resolve(260., 10., 7).ticks, 3);
        // Never below three, and never above what the label asked for.
        assert_eq!(Degrade::resolve(260., 2., 7).ticks, 3);
        assert_eq!(Degrade::resolve(260., 150., 5).ticks, 5);
    }

    // ---- the mark ------------------------------------------------------

    #[test]
    fn an_unknown_provider_still_gets_a_mark() {
        assert_eq!(initials("Claude Code", "claude"), "CC");
        assert_eq!(initials("Codex", "codex"), "CO");
        assert_eq!(initials("", "fireworks"), "FI");
        assert_eq!(initials("", ""), "");
    }

    // ---- the rollup ----------------------------------------------------

    #[test]
    fn the_rollup_knows_when_to_ask_for_a_human() {
        let quiet = Tally {
            working: 2,
            idle: 9,
            finished: 4,
            ..Default::default()
        };
        assert!(!quiet.needs_you());
        assert_eq!(quiet.total(), 15);

        let waiting = Tally {
            blocked: 1,
            ..quiet
        };
        assert!(waiting.needs_you());

        let broken = Tally {
            errored: 1,
            ..quiet
        };
        assert!(broken.needs_you());
    }

    // ---- reading a record into a row -----------------------------------

    fn rec(id: &str, limits: Vec<(&str, f32)>) -> crate::usage::Record {
        crate::usage::Record {
            id: id.into(),
            name: "Claude Code".into(),
            ready: true,
            limits: limits
                .into_iter()
                .map(|(label, percent)| crate::usage::Limit {
                    label: label.into(),
                    percent,
                    resets_at: String::new(),
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_provider_with_no_collector_reads_unknown_on_both_rails() {
        // `ready: false` is the collector saying it has nothing to report. The
        // failure being guarded is drawing that as two exhausted bars.
        let mut r = rec("claude", vec![("Session (5-hour)", 0.09)]);
        r.ready = false;
        let row = ProviderRow::from_record(&r, Some(0.1));
        assert!(row.session.is_unknown());
        assert!(row.week.is_unknown());
        assert_eq!(row.session.spent(), None);
    }

    #[test]
    fn an_expired_sign_in_reads_unknown_even_with_numbers_present() {
        let mut r = rec("claude", vec![("Session (5-hour)", 0.09)]);
        r.status_text = "Sign-in expired".into();
        let row = ProviderRow::from_record(&r, Some(0.1));
        assert!(
            row.session.is_unknown(),
            "a stale sign-in's numbers are not current"
        );
    }

    #[test]
    fn a_vendor_that_publishes_no_weekly_window_leaves_the_week_unknown() {
        // The week must not borrow the session's number to have something to
        // draw — that is the same collapse in a different costume.
        let r = rec("claude", vec![("Session (5-hour)", 0.09)]);
        let row = ProviderRow::from_record(&r, Some(0.1));
        assert_eq!(row.session.spent(), Some(0.09));
        assert!(row.week.is_unknown());
    }

    #[test]
    fn both_windows_read_when_both_are_published() {
        let r = rec(
            "claude",
            vec![("Session (5-hour)", 0.09), ("Weekly (7-day)", 0.87)],
        );
        let row = ProviderRow::from_record(&r, Some(0.5));
        assert_eq!(row.session.spent(), Some(0.09));
        assert_eq!(row.week.spent(), Some(0.87));
        assert_eq!(row.session_ticks, 5, "a tick is an hour");
        assert_eq!(row.initials, "CC");
    }

    #[test]
    fn a_record_older_than_its_own_window_is_stale_not_fresh() {
        let r = rec("claude", vec![("Session (5-hour)", 0.4)]);
        let fresh = ProviderRow::from_record(&r, Some(2.0));
        assert!(
            !fresh.session.is_stale(),
            "two hours into a five-hour window"
        );

        let old = ProviderRow::from_record(&r, Some(6.0));
        assert!(old.session.is_stale(), "the window has since rolled over");
        assert_eq!(old.session.spent(), Some(0.4), "the number is kept");
        assert_eq!(old.session.age(), Some("6h"));
    }

    #[test]
    fn a_record_that_does_not_say_when_it_was_written_is_not_assumed_current() {
        // Undeclared is not none: an absent `updatedAt` is one more thing
        // nobody measured, and reading it as "just now" is the cheap wrong
        // number this house has already shipped twice.
        let r = rec("claude", vec![("Session (5-hour)", 0.4)]);
        let row = ProviderRow::from_record(&r, None);
        assert!(row.session.is_stale());
        assert_eq!(row.session.age(), Some("?"));
    }

    #[test]
    fn unlabelled_limits_fall_back_to_position_and_no_further() {
        let r = rec("some-vendor", vec![("Allowance", 0.2)]);
        let row = ProviderRow::from_record(&r, Some(0.1));
        assert_eq!(
            row.session.spent(),
            Some(0.2),
            "the only one is the session"
        );
        assert!(row.week.is_unknown(), "and the week is still unknown");
        assert_eq!(row.session_ticks, 6, "no unit named, so six");
    }

    #[test]
    fn the_window_span_is_read_for_the_staleness_check_only() {
        assert_eq!(window_hours("Session (5-hour)"), Some(5.0));
        assert_eq!(window_hours("Weekly (7-day)"), Some(168.0));
        assert_eq!(window_hours("Weekly"), Some(168.0));
        assert_eq!(window_hours("Prepaid balance"), None);
    }

    #[test]
    fn labels_sort_themselves_into_the_two_rails() {
        assert!(looks_like_session("Session (5-hour)"));
        assert!(looks_like_session("Current 5h window"));
        assert!(!looks_like_session("Weekly (7-day)"));
        assert!(looks_like_week("Weekly (7-day)"));
        assert!(looks_like_week("Monthly"));
        assert!(!looks_like_week("Session (5-hour)"));
    }
}
