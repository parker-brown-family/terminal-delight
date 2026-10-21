//! Old glyphs fade at their old screen positions; the current grid is never delayed.
use super::*;
use std::collections::{BTreeMap, VecDeque};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(super) const MIN_GLYPH_BUDGET: usize = 4096;

#[derive(Clone)]
pub(super) struct Ghost {
    pub row: usize,
    pub column: usize,
    pub text: String,
    pub run: TextRun,
    pub line_columns: usize,
    pub leading_columns: usize,
    born: Instant,
    lifetime: Duration,
    erased: bool,
}

impl Ghost {
    pub fn opacity(&self, now: Instant) -> f32 {
        let remaining = 1.0
            - now.saturating_duration_since(self.born).as_secs_f32() / self.lifetime.as_secs_f32();
        let remaining = remaining.clamp(0.0, 1.0);
        0.65 * if self.erased {
            remaining
        } else {
            remaining * remaining
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct Trail {
    pub glyphs: VecDeque<Ghost>,
}

/// Adjacent retained cells share layout, while each style keeps its own fade.
/// Layer order and the old line's crawl origin remain authoritative.
pub(super) struct PaintGroup {
    pub row: usize,
    pub column: usize,
    pub text: String,
    pub runs: Vec<TextRun>,
    pub line_columns: usize,
    pub leading_columns: usize,
    columns: usize,
}

fn columns(text: &str) -> impl Iterator<Item = (usize, usize, &str)> {
    text.grapheme_indices(true)
        .scan(0, |column, (byte, glyph)| {
            let at = *column;
            *column += UnicodeWidthStr::width(glyph);
            Some((at, byte, glyph))
        })
}

fn lifetime(timing: theme::TextLinger, erased: bool) -> Option<Duration> {
    let (ms, max) = if erased {
        (timing.off_cursor_ms, theme::TextLinger::OFF_CURSOR_MAX_MS)
    } else {
        (timing.cursor_ms, theme::TextLinger::CURSOR_MAX_MS)
    };
    (ms.is_finite() && ms > 0.0)
        .then(|| Duration::from_secs_f64(f64::from(ms.min(max)) / 1000.0))
        .filter(|duration| !duration.is_zero())
}

impl Trail {
    /// A dial changes existing trails without renewing their original age.
    /// A zero removes only that category, leaving the other one intact.
    pub fn retime(&mut self, timing: theme::TextLinger, now: Instant) {
        self.glyphs.retain_mut(|ghost| {
            let Some(duration) = lifetime(timing, ghost.erased) else {
                return false;
            };
            ghost.lifetime = duration;
            now.saturating_duration_since(ghost.born) < duration
        });
    }
    pub fn paint_groups(&self, now: Instant) -> Vec<PaintGroup> {
        let mut groups: Vec<PaintGroup> = Vec::new();
        for ghost in &self.glyphs {
            let width = UnicodeWidthStr::width(ghost.text.as_str());
            let color = ghost
                .run
                .color
                .alpha(ghost.run.color.a * ghost.opacity(now));
            let adjacent = groups.last_mut().filter(|group| {
                width > 0
                    && group.columns > 0
                    && group.row == ghost.row
                    && group.column + group.columns == ghost.column
                    && group.line_columns == ghost.line_columns
                    && group.leading_columns == ghost.leading_columns
            });
            if let Some(group) = adjacent {
                group.text.push_str(&ghost.text);
                group.columns += width;
                if let Some(run) = group
                    .runs
                    .last_mut()
                    .filter(|run| run.font == ghost.run.font && run.color == color)
                {
                    run.len += ghost.text.len();
                } else {
                    group.runs.push(TextRun {
                        color,
                        ..ghost.run.clone()
                    });
                }
            } else {
                groups.push(PaintGroup {
                    row: ghost.row,
                    column: ghost.column,
                    text: ghost.text.clone(),
                    runs: vec![TextRun {
                        color,
                        ..ghost.run.clone()
                    }],
                    line_columns: ghost.line_columns,
                    leading_columns: ghost.leading_columns,
                    columns: width,
                });
            }
        }
        groups
    }

    pub fn clear(&mut self) {
        self.glyphs.clear();
    }

    pub fn prune(&mut self, now: Instant) {
        self.glyphs
            .retain(|g| now.saturating_duration_since(g.born) < g.lifetime);
    }

    pub fn observe(
        &mut self,
        previous: &styling::Lines,
        current: &styling::Lines,
        now: Instant,
        timing: theme::TextLinger,
        glyph_budget: usize,
    ) {
        let cursor_duration = lifetime(timing, false);
        let erased_duration = lifetime(timing, true);
        if glyph_budget == 0 || (cursor_duration.is_none() && erased_duration.is_none()) {
            self.clear();
            return;
        }
        self.prune(now);
        // A row losing visible graphemes is an erasure of this image,
        // regardless of which editor command caused it. Text added on another
        // row must not cancel the erased word's longer lifetime.
        // All changed old letters share that lifetime, including ones whose
        // positions are now occupied by a moved prompt or another live line.
        let ink = |s: &str| {
            s.graphemes(true)
                .filter(|g| !g.chars().all(char::is_whitespace))
                .count()
        };
        let erased = (0..previous.len().max(current.len())).any(|row| {
            let old = previous.get(row).map_or("", |(s, _)| s.as_str());
            let new = current.get(row).map_or("", |(s, _)| s.as_str());
            old != new && ink(old) > ink(new)
        });
        let duration = if erased {
            erased_duration
        } else {
            cursor_duration
        };
        for (row, (old, runs)) in previous.iter().enumerate() {
            let new = current.get(row).map_or("", |(s, _)| s.as_str());
            if old == new {
                continue; // focus, cursor and selection colour changes emit no trail
            }
            let new_columns: BTreeMap<_, _> = columns(new).map(|(col, _, g)| (col, g)).collect();
            // A returning live letter must not be drawn twice over its own trail.
            self.glyphs.retain(|g| {
                g.row != row || new_columns.get(&g.column).copied() != Some(g.text.as_str())
            });
            // Suppression of returning live letters still applies when this
            // category is disabled; it must not leave a stale older ghost.
            let Some(lifetime) = duration else {
                continue;
            };
            let (mut run_index, mut run_end) = (0, runs.first().map_or(0, |r| r.len));
            let line_columns = UnicodeWidthStr::width(old.trim());
            let leading_columns =
                UnicodeWidthStr::width(old.as_str()) - UnicodeWidthStr::width(old.trim_start());
            for (column, byte, glyph) in columns(old) {
                while byte >= run_end && run_index + 1 < runs.len() {
                    run_index += 1;
                    run_end += runs[run_index].len;
                }
                if glyph.chars().all(char::is_whitespace)
                    || new_columns.get(&column).copied() == Some(glyph)
                {
                    continue;
                }
                let Some(run) = runs.get(run_index) else {
                    continue;
                };
                while self.glyphs.len() >= glyph_budget {
                    self.glyphs.pop_front();
                }
                self.glyphs.push_back(Ghost {
                    row,
                    column,
                    text: glyph.to_owned(),
                    line_columns,
                    leading_columns,
                    run: TextRun {
                        len: glyph.len(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                        ..run.clone()
                    },
                    born: now,
                    lifetime,
                    erased,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timing(ms: f32) -> theme::TextLinger {
        theme::TextLinger {
            cursor_ms: ms,
            off_cursor_ms: if ms > 0.0 { 650.0 } else { 0.0 },
        }
    }

    fn lines(s: &str) -> styling::Lines {
        vec![(
            s.into(),
            vec![TextRun {
                len: s.len(),
                font: font("monospace"),
                color: rgb(0x73ffb0).into(),
                background_color: Some(rgb(0x002200).into()),
                underline: Some(Default::default()),
                strikethrough: Some(Default::default()),
            }],
        )]
    }

    #[test]
    fn erased_text_obeys_every_selected_time_without_a_hidden_minimum() {
        let now = Instant::now();
        for ms in [0, 1, 125, 325, 650] {
            let mut trail = Trail::default();
            trail.observe(
                &lines("ENTIRE DELETED STRING"),
                &lines(""),
                now,
                theme::TextLinger {
                    cursor_ms: 220.0,
                    off_cursor_ms: ms as f32,
                },
                MIN_GLYPH_BUDGET,
            );
            if ms == 0 {
                assert!(trail.glyphs.is_empty());
                continue;
            }
            assert_eq!(
                trail
                    .glyphs
                    .iter()
                    .map(|g| g.text.as_str())
                    .collect::<String>(),
                "ENTIREDELETEDSTRING"
            );
            trail.prune(now + Duration::from_millis(ms - 1));
            assert!(!trail.glyphs.is_empty(), "{ms}ms expired early");
            trail.prune(now + Duration::from_millis(ms));
            assert!(trail.glyphs.is_empty(), "{ms}ms ignored the slider");
        }
    }

    #[test]
    fn retiming_preserves_birth_and_disabling_one_category_keeps_the_other() {
        let now = Instant::now();
        let maximum = timing(220.0);
        let mut trail = Trail::default();
        trail.observe(&lines("AB"), &lines("A"), now, maximum, MIN_GLYPH_BUDGET);
        trail.observe(&lines("A"), &lines("C"), now, maximum, MIN_GLYPH_BUDGET);
        assert_eq!(trail.glyphs.len(), 2);
        let original_births: Vec<_> = trail.glyphs.iter().map(|g| g.born).collect();
        let shorter = theme::TextLinger {
            cursor_ms: 100.0,
            off_cursor_ms: 400.0,
        };
        trail.retime(shorter, now + Duration::from_millis(50));
        assert_eq!(
            trail.glyphs.iter().map(|g| g.born).collect::<Vec<_>>(),
            original_births
        );
        assert_eq!(trail.glyphs[0].lifetime, Duration::from_millis(400));
        assert_eq!(trail.glyphs[1].lifetime, Duration::from_millis(100));
        for disable_erased in [false, true] {
            let mut changed = trail.clone();
            let timing = if disable_erased {
                theme::TextLinger {
                    off_cursor_ms: 0.0,
                    ..shorter
                }
            } else {
                theme::TextLinger {
                    cursor_ms: 0.0,
                    ..shorter
                }
            };
            changed.retime(timing, now + Duration::from_millis(60));
            assert_eq!(changed.glyphs.len(), 1);
            assert_eq!(changed.glyphs[0].erased, !disable_erased);
            assert_eq!(changed.glyphs[0].born, now);
        }
        trail.retime(shorter, now + Duration::from_millis(100));
        assert_eq!(trail.glyphs.len(), 1);
        trail.retime(shorter, now + Duration::from_millis(400));
        assert!(trail.glyphs.is_empty());
    }

    #[test]
    fn a_disabled_text_change_cannot_clear_an_independent_erased_trail() {
        let now = Instant::now();
        let timing = theme::TextLinger {
            cursor_ms: 0.0,
            off_cursor_ms: 500.0,
        };
        let mut trail = Trail::default();
        trail.observe(&lines("AB"), &lines("A"), now, timing, MIN_GLYPH_BUDGET);
        trail.observe(&lines("A"), &lines("C"), now, timing, MIN_GLYPH_BUDGET);
        assert_eq!(trail.glyphs.len(), 1);
        assert_eq!(trail.glyphs[0].text, "B");
        // A returning letter removes its own older image, even with short linger off.
        trail.observe(&lines("CC"), &lines("CB"), now, timing, MIN_GLYPH_BUDGET);
        assert!(trail.glyphs.is_empty());
        for invalid in [f32::NAN, f32::INFINITY, -1.0, f32::MIN_POSITIVE] {
            assert!(lifetime(
                theme::TextLinger {
                    cursor_ms: invalid,
                    off_cursor_ms: invalid
                },
                false
            )
            .is_none());
        }
    }

    #[test]
    fn a_full_erased_viewport_uses_one_layout_group_per_row() {
        let now = Instant::now();
        let row = lines(&"X".repeat(150)).pop().unwrap();
        let old = vec![row; 36];
        let mut trail = Trail::default();
        trail.observe(&old, &Vec::new(), now, timing(220.0), 150 * 36);
        assert_eq!(trail.glyphs.len(), 5400);
        let groups = trail.paint_groups(now + Duration::from_millis(500));
        assert_eq!(groups.len(), 36, "layout still scales per glyph");
        for (row, group) in groups.iter().enumerate() {
            assert_eq!((group.row, group.column, group.columns), (row, 0, 150));
            assert_eq!(group.text, "X".repeat(150));
            assert_eq!(group.runs.len(), 1);
            assert_eq!(group.runs[0].len, 150);
            assert!(group.runs[0].color.a > 0.0);
        }
        trail.prune(now + Duration::from_millis(650));
        assert!(trail
            .paint_groups(now + Duration::from_millis(650))
            .is_empty());
    }

    #[test]
    fn batching_preserves_wide_combining_spaces_colours_and_fade_ages() {
        let now = Instant::now();
        let mut old = lines("A 界e\u{301}Z");
        let first = TextRun {
            len: 2,
            ..old[0].1[0].clone()
        };
        let second = TextRun {
            len: old[0].0.len() - 2,
            color: rgb(0xff7700).into(),
            ..old[0].1[0].clone()
        };
        old[0].1 = vec![first, second];
        let mut trail = Trail::default();
        trail.observe(&old, &Vec::new(), now, timing(220.0), MIN_GLYPH_BUDGET);
        let groups = trail.paint_groups(now + Duration::from_millis(500));
        assert_eq!(groups.len(), 2, "a blank column must remain a gap");
        assert_eq!((groups[0].column, groups[0].text.as_str()), (0, "A"));
        assert_eq!(
            (groups[1].column, groups[1].text.as_str(), groups[1].columns),
            (2, "界e\u{301}Z", 4)
        );
        for group in &groups {
            assert_eq!(
                group.runs.iter().map(|r| r.len).sum::<usize>(),
                group.text.len()
            );
            assert_eq!(group.line_columns, 6);
            assert_eq!(group.leading_columns, 0);
            assert!(group.runs.iter().all(|r| r.background_color.is_none()
                && r.underline.is_none()
                && r.strikethrough.is_none()));
        }
        assert_eq!(groups[0].runs[0].color.h, trail.glyphs[0].run.color.h);
        assert_eq!(groups[1].runs[0].color.h, trail.glyphs[1].run.color.h);

        // Two generations at the same cells remain two ordered layers.
        let mut trail = Trail::default();
        trail.observe(
            &lines("ABC"),
            &lines("XYZ"),
            now,
            timing(220.0),
            MIN_GLYPH_BUDGET,
        );
        trail.observe(
            &lines("XYZ"),
            &lines("---"),
            now + Duration::from_millis(50),
            timing(220.0),
            MIN_GLYPH_BUDGET,
        );
        let groups = trail.paint_groups(now + Duration::from_millis(100));
        assert_eq!(
            groups.iter().map(|g| g.text.as_str()).collect::<Vec<_>>(),
            ["ABC", "XYZ"]
        );
        assert!(groups[0].runs[0].color.a < groups[1].runs[0].color.a);
    }

    #[test]
    fn batching_does_not_join_distinct_old_crawl_origins() {
        let now = Instant::now();
        let mut trail = Trail::default();
        trail.observe(
            &lines("AB"),
            &lines("XB"),
            now,
            timing(220.0),
            MIN_GLYPH_BUDGET,
        );
        trail.observe(
            &lines("XBC"),
            &lines("XYC"),
            now,
            timing(220.0),
            MIN_GLYPH_BUDGET,
        );
        let groups = trail.paint_groups(now);
        assert_eq!(groups.len(), 2);
        assert_eq!((groups[0].column, groups[0].line_columns), (0, 2));
        assert_eq!((groups[1].column, groups[1].line_columns), (1, 3));
    }

    #[test]
    fn only_replaced_glyphs_emit_and_current_text_stays_untouched() {
        let now = Instant::now();
        let (old, current) = (lines("OLD same"), lines("NEW same"));
        let before = current.clone();
        let mut trail = Trail::default();
        trail.observe(&old, &current, now, timing(200.0), MIN_GLYPH_BUDGET);
        assert_eq!(
            trail
                .glyphs
                .iter()
                .map(|g| g.text.as_str())
                .collect::<String>(),
            "OLD"
        );
        assert_eq!(current, before);
        assert!(trail.glyphs.iter().all(|g| g.run.background_color.is_none()
            && g.run.underline.is_none()
            && g.run.strikethrough.is_none()));
    }

    #[test]
    fn fading_is_time_based_and_stops_even_without_more_output() {
        let now = Instant::now();
        let mut trail = Trail::default();
        trail.observe(
            &lines("A"),
            &lines("B"),
            now,
            timing(200.0),
            MIN_GLYPH_BUDGET,
        );
        let ghost = &trail.glyphs[0];
        assert!((ghost.opacity(now) - 0.65).abs() < 0.001);
        assert!((ghost.opacity(now + Duration::from_millis(100)) - 0.1625).abs() < 0.001);
        trail.prune(now + Duration::from_millis(200));
        assert!(trail.glyphs.is_empty());
    }

    #[test]
    fn zero_clears_history_and_style_only_changes_do_not_emit() {
        let now = Instant::now();
        let mut trail = Trail::default();
        let mut recoloured = lines("A");
        recoloured[0].1[0].color = rgb(0xff0033).into();
        trail.observe(
            &lines("A"),
            &recoloured,
            now,
            timing(200.0),
            MIN_GLYPH_BUDGET,
        );
        assert!(trail.glyphs.is_empty());
        trail.observe(
            &lines("A"),
            &lines("B"),
            now,
            timing(200.0),
            MIN_GLYPH_BUDGET,
        );
        trail.retime(timing(0.0), now);
        trail.observe(&lines("B"), &lines("C"), now, timing(0.0), MIN_GLYPH_BUDGET);
        assert!(trail.glyphs.is_empty());
    }

    #[test]
    fn wide_and_combining_glyphs_keep_their_terminal_columns() {
        let now = Instant::now();
        let mut trail = Trail::default();
        trail.observe(
            &lines("界e\u{301}Z"),
            &lines("界xZ"),
            now,
            timing(200.0),
            MIN_GLYPH_BUDGET,
        );
        assert_eq!(trail.glyphs.len(), 1);
        assert_eq!(trail.glyphs[0].text, "e\u{301}");
        assert_eq!(trail.glyphs[0].column, 2);
        assert_eq!(trail.glyphs[0].run.len, "e\u{301}".len());
    }

    #[test]
    fn a_live_letter_replaces_its_own_older_ghost() {
        let now = Instant::now();
        let mut trail = Trail::default();
        trail.observe(
            &lines("A"),
            &lines("B"),
            now,
            timing(200.0),
            MIN_GLYPH_BUDGET,
        );
        trail.observe(
            &lines("B"),
            &lines("A"),
            now,
            timing(200.0),
            MIN_GLYPH_BUDGET,
        );
        assert_eq!(trail.glyphs.len(), 1);
        assert_eq!(trail.glyphs[0].text, "B");
    }

    #[test]
    fn history_has_a_hard_per_pane_budget() {
        let now = Instant::now();
        let mut trail = Trail::default();
        trail.observe(
            &lines(&"X".repeat(MIN_GLYPH_BUDGET + 30)),
            &lines(""),
            now,
            timing(200.0),
            MIN_GLYPH_BUDGET,
        );
        assert_eq!(trail.glyphs.len(), MIN_GLYPH_BUDGET);
        trail.clear();
        assert!(trail.glyphs.is_empty());
    }

    #[test]
    fn an_entire_erased_word_lingers_for_650ms_without_renewing_on_cursor_changes() {
        let now = Instant::now();
        let mut trail = Trail::default();
        trail.observe(
            &lines("whole word"),
            &lines("whole "),
            now,
            timing(220.0),
            MIN_GLYPH_BUDGET,
        );
        assert_eq!(
            trail
                .glyphs
                .iter()
                .map(|g| g.text.as_str())
                .collect::<String>(),
            "word"
        );
        let born = trail.glyphs[0].born;
        let mut cursor = lines("whole ");
        cursor[0].1[0].color = rgb(0xff0033).into();
        trail.observe(
            &lines("whole "),
            &cursor,
            now + Duration::from_millis(200),
            timing(220.0),
            MIN_GLYPH_BUDGET,
        );
        assert_eq!(trail.glyphs[0].born, born);
        trail.prune(now + Duration::from_millis(300));
        assert_eq!(trail.glyphs.len(), 4);
        assert!(trail.glyphs[0].opacity(now + Duration::from_millis(500)) > 0.14);
        trail.prune(now + Duration::from_millis(650));
        assert!(trail.glyphs.is_empty());
    }

    #[test]
    fn all_changed_letters_of_an_erased_paragraph_share_the_longer_fade() {
        let now = Instant::now();
        let mut previous = lines("The first line");
        previous.extend(lines("The second line"));
        previous.extend(lines("The final line"));
        let mut trail = Trail::default();
        trail.observe(&previous, &lines(">"), now, timing(220.0), MIN_GLYPH_BUDGET);
        assert_eq!(
            trail
                .glyphs
                .iter()
                .map(|g| g.text.as_str())
                .collect::<String>(),
            "ThefirstlineThesecondlineThefinalline"
        );
        assert!(trail
            .glyphs
            .iter()
            .all(|g| g.lifetime == Duration::from_millis(650)));
        trail.prune(now + Duration::from_millis(300));
        assert!(!trail.glyphs.is_empty());
        trail.retime(timing(0.0), now + Duration::from_millis(310));
        trail.observe(
            &lines(">"),
            &lines("new"),
            now + Duration::from_millis(310),
            timing(0.0),
            MIN_GLYPH_BUDGET,
        );
        assert!(trail.glyphs.is_empty());
    }

    #[test]
    fn a_whole_viewport_fits_even_when_it_exceeds_the_small_pane_budget() {
        let now = Instant::now();
        let viewport = 100 * 60;
        let mut trail = Trail::default();
        trail.observe(
            &lines(&"X".repeat(viewport)),
            &lines(""),
            now,
            timing(220.0),
            viewport,
        );
        assert_eq!(trail.glyphs.len(), viewport);
        trail.observe(&lines("Y"), &lines("Z"), now, timing(220.0), viewport);
        assert_eq!(trail.glyphs.len(), viewport);
    }

    #[test]
    fn text_added_on_another_row_cannot_cancel_a_word_erasure() {
        let now = Instant::now();
        let mut previous = lines("");
        previous.extend(lines("word"));
        let mut current = lines("A newly restored heading");
        current.extend(lines(""));
        let mut trail = Trail::default();
        trail.observe(&previous, &current, now, timing(220.0), MIN_GLYPH_BUDGET);
        assert_eq!(trail.glyphs.len(), 4);
        assert!(trail
            .glyphs
            .iter()
            .all(|g| g.lifetime == Duration::from_millis(650)));
    }
}
