//! Old glyphs fade at their old screen positions; the current grid is never delayed.
use super::*;
use std::collections::{BTreeMap, VecDeque};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(super) const MAX_GLYPHS: usize = 4096;

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
}

impl Ghost {
    pub fn opacity(&self, now: Instant) -> f32 {
        let remaining = 1.0
            - now.saturating_duration_since(self.born).as_secs_f32() / self.lifetime.as_secs_f32();
        0.65 * remaining.clamp(0.0, 1.0).powi(2)
    }
}

#[derive(Clone, Default)]
pub(super) struct Trail {
    pub glyphs: VecDeque<Ghost>,
}

fn columns(text: &str) -> impl Iterator<Item = (usize, usize, &str)> {
    text.grapheme_indices(true)
        .scan(0, |column, (byte, glyph)| {
            let at = *column;
            *column += UnicodeWidthStr::width(glyph);
            Some((at, byte, glyph))
        })
}

impl Trail {
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
        milliseconds: f32,
    ) {
        if !milliseconds.is_finite() || milliseconds <= 0.0 {
            self.clear();
            return;
        }
        self.prune(now);
        let lifetime = Duration::from_secs_f64(f64::from(milliseconds.min(1200.0)) / 1000.0);
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
                while self.glyphs.len() >= MAX_GLYPHS {
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
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn only_replaced_glyphs_emit_and_current_text_stays_untouched() {
        let now = Instant::now();
        let (old, current) = (lines("OLD same"), lines("NEW same"));
        let before = current.clone();
        let mut trail = Trail::default();
        trail.observe(&old, &current, now, 200.0);
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
        trail.observe(&lines("A"), &lines("B"), now, 200.0);
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
        trail.observe(&lines("A"), &recoloured, now, 200.0);
        assert!(trail.glyphs.is_empty());
        trail.observe(&lines("A"), &lines("B"), now, 200.0);
        trail.observe(&lines("B"), &lines("C"), now, 0.0);
        assert!(trail.glyphs.is_empty());
    }

    #[test]
    fn wide_and_combining_glyphs_keep_their_terminal_columns() {
        let now = Instant::now();
        let mut trail = Trail::default();
        trail.observe(&lines("界e\u{301}Z"), &lines("界xZ"), now, 200.0);
        assert_eq!(trail.glyphs.len(), 1);
        assert_eq!(trail.glyphs[0].text, "e\u{301}");
        assert_eq!(trail.glyphs[0].column, 2);
        assert_eq!(trail.glyphs[0].run.len, "e\u{301}".len());
    }

    #[test]
    fn a_live_letter_replaces_its_own_older_ghost() {
        let now = Instant::now();
        let mut trail = Trail::default();
        trail.observe(&lines("A"), &lines("B"), now, 200.0);
        trail.observe(&lines("B"), &lines("A"), now, 200.0);
        assert_eq!(trail.glyphs.len(), 1);
        assert_eq!(trail.glyphs[0].text, "B");
    }

    #[test]
    fn history_has_a_hard_per_pane_budget() {
        let now = Instant::now();
        let mut trail = Trail::default();
        trail.observe(&lines(&"X".repeat(MAX_GLYPHS + 30)), &lines(""), now, 200.0);
        assert_eq!(trail.glyphs.len(), MAX_GLYPHS);
        trail.clear();
        assert!(trail.glyphs.is_empty());
    }
}
