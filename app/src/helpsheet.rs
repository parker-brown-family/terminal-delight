//! The help sheet's content and its filter, apart from how it is drawn.
//!
//! The sheet used to be two hand-built columns of `row(...)` calls inside the
//! render function, so a new chord meant finding the right column by eye and
//! the sheet could not be searched. It is now a list of sections, built once
//! per frame from the language pack, which the render lays out and the filter
//! narrows.
//!
//! **The filter** answers what Parker asked for on 2026-11-03: *"TYPING (no
//! click just a raw 'word' will FUZZY FILTER live the options) really similar
//! to omarchy hotkeys … include SECTIONS in the filter"*. Every word typed must
//! be found in the row's key, its description or its section's title. A word
//! counts as found when it is a substring, or when its letters appear in order
//! within a short stretch (`gtab` finds "Ctrl+Shift+T … tab"). A plain
//! subsequence over the whole line would let three letters match nearly every
//! row, which is a filter that does not filter.
//!
//! While a query is typed the sheet searches SHORTCUTS and FEATURES together:
//! someone typing "mark" wants the chord and the feature, and should not have
//! to know which tab it lives on first.

use crate::lang::Strings;

/// One line: what to press (or a feature's name) and what it does.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub key: String,
    pub desc: String,
}

/// A titled block of rows. `feature` marks the FEATURES tour's sections, so a
/// filtered view that mixes both can tell them apart.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub title: String,
    pub rows: Vec<Row>,
    pub feature: bool,
}

fn row(key: impl Into<String>, desc: impl Into<String>) -> Row {
    Row {
        key: key.into(),
        desc: desc.into(),
    }
}

fn section(title: impl Into<String>, rows: Vec<Row>) -> Section {
    Section {
        title: title.into(),
        rows,
        feature: false,
    }
}

/// The SHORTCUTS view, in reading order: the two halves of the sheet are cut
/// from this list by [`columns`], so order here is order on screen.
pub fn shortcuts(s: &Strings) -> Vec<Section> {
    let h = s.hs;
    vec![
        section(
            s.s_tabs,
            vec![
                row("Ctrl+Shift+T", s.new_tab),
                row("Ctrl+PgUp / PgDn", s.switch_tabs),
                row("Ctrl+Shift+PgUp / PgDn", s.move_tab),
                row("Alt+V / H · Ctrl+Alt+D", s.split),
                row("Alt+W", s.close_pane),
                row("Ctrl+W", s.close_doc),
                row("Ctrl+Shift+W", s.close_tab),
                row("Ctrl+Shift+Z", s.reopen_closed),
                row("Ctrl+Alt+R", s.rename_row),
                row(s.k_drag_subtab, s.drag_subtab),
                row(s.k_rclick_tab, s.rclick_tab),
                row("Ctrl+Shift+B", h.left_bar),
                row("Alt+K", s.toggle_bench),
            ],
        ),
        section(
            h.s_around,
            vec![
                row(s.k_alt_arrows, s.move_focus_dir),
                row("Alt+1 … 9", s.help_marks),
                row("Alt+0 / Alt+Shift+0", s.help_back_fwd),
                row(h.k_mouse_nav, s.help_back_fwd),
                row("Alt+Shift+A … Z", h.pin_tab),
                row("Alt+L", s.lookup),
                row("Alt+O", s.help_open_artifact),
                row("Alt+Q · Ctrl+Shift+N", s.rail),
                row("Alt+Q → 1 … 9", h.needs_digit),
                row("Ctrl+Alt+↑↓←→", s.walk_tree),
                row("Ctrl+Alt+1…9", s.jump_branch),
                row("Alt+G", s.git_coordination),
            ],
        ),
        section(
            s.s_edit,
            vec![
                row(s.k_rclick, s.rclick),
                row("Ctrl+Shift+C / V", s.copy_paste),
                row("Ctrl+Insert / Shift+Insert", s.copy_paste),
                row("Ctrl+X", s.cut),
                row("Shift+Backspace", s.delete_word),
                row("Ctrl+Shift+Backspace", s.delete_line),
                row("Ctrl+Shift+Alt+Backspace", s.clear_input),
                row(s.k_ctrl_arrows, s.word_jump),
                row("Ctrl+F", s.find),
                row("Ctrl+Shift+F", s.find_all),
                row(s.k_dbl_click, s.select_wl),
                row("Shift+Enter", s.newline),
            ],
        ),
        section(
            s.s_links,
            vec![
                row(s.k_ctrl_click, s.open_link),
                row(s.k_shift_click, s.reveal_link),
                row(s.k_super_ctrl_click, s.reveal_link),
                row(s.k_alt_click, s.open_here),
                row(s.k_ctrl_alt_click, s.open_beside),
            ],
        ),
        section(
            s.s_scroll,
            vec![
                row(s.k_scroll_wheel, s.scroll_hist),
                row("Shift+PgUp / PgDn", s.scroll_hist),
                row("PgUp / PgDn", h.ag_page),
                row("Ctrl+Home / End", h.ag_ends),
                row("Alt+E / Alt+Shift+E", s.help_next_error),
                row("Alt+[", s.help_scroll_mode),
                row("Ctrl+Shift+K", s.clear_scroll),
                row("Ctrl+↑↓ · Ctrl+Shift+↑↓", s.turn_square),
            ],
        ),
        section(
            format!("{} · Alt+[", h.s_scroll_mode),
            vec![
                row("j / k · u / d", h.sm_line_page),
                row("g / G", h.sm_ends),
                row("[ / ]", h.sm_block),
                row("{ / }", h.sm_turn),
                row("e / E", h.sm_error),
                row("/", h.sm_find),
                row("q · Esc", h.sm_exit),
            ],
        ),
        // Inside a document is where the keyboard changes most: plain letters
        // stop typing and start reading (`keylayer::read_key`), so it gets its
        // own block rather than footnotes on the rows above.
        section(
            h.s_reading,
            vec![
                row("j / k · ↑ ↓", h.r_line),
                row("Space / Shift+Space", h.r_page),
                row("g / Shift+G", h.r_ends),
                row("Tab / Shift+Tab", h.r_target),
                row("] / [", h.r_decision),
                row("Enter", h.r_note),
                row("c", h.r_concur),
                row("y", h.r_copy_map),
                row("Ctrl+Shift+Enter", h.r_send),
                row("+ / − · 0 · 1", h.r_zoom),
                row("Esc", h.r_escape),
                row("Alt+O", h.r_list),
                row("Ctrl+W", s.close_doc),
            ],
        ),
        section(
            s.s_agents,
            vec![
                row("Alt+A", s.launch_agent),
                row("Ctrl+↑ / ↓", s.jump_msg),
                row("Alt+R", s.focus),
                row(s.k_focus_inherit_key, s.focus_inherit),
                row("Alt+1 … 4 (marks off)", s.bench_shelf),
                row("Alt+M", s.bench_note),
                row("Alt+S", s.sticky),
                row("Alt+Backspace", s.sticky_peel),
                row(s.k_rclick_note, s.sticky_pin),
                row(s.k_wheel_key, s.pan_focus),
                row(s.k_input_colour, s.input_colour),
                row("Ctrl+Shift+M", s.handed_over),
                row(
                    format!("\u{f06a9} {}", s.ui.floor_counter_key),
                    s.ui.floor_help,
                ),
                row("Ctrl+Shift+A", h.agent_watch),
                row("Ctrl+Shift+U / Y", s.usage),
            ],
        ),
        section(
            s.s_look,
            vec![
                row(s.k_theme_icon, s.themes_wheel),
                row("Ctrl+Shift+D", s.themes_wheel),
                row(format!("⛭ {}", s.k_display_tray), s.display_tray),
                row("Ctrl+Shift+G", s.display_tray),
                row(s.ui.help_key_ctrl_wheel, s.outer_size),
                row("Ctrl+Shift+= / -", s.outer_size_key),
                row(s.k_text_size_key, s.text_size),
                row("Ctrl+= / -", s.text_size),
                row(s.k_warp_dial, s.warp),
            ],
        ),
        section(
            s.s_window,
            vec![row("Ctrl+Alt+T", s.new_window), row("F1", s.help)],
        ),
    ]
}

/// The FEATURES tour: what Terminal Delight can do that no single chord
/// explains. Rebuilt for 0.5.0 from the changelog and the docs site.
pub fn features(s: &Strings) -> Vec<Section> {
    let h = s.hs;
    let feature = |title: &str, rows: Vec<Row>| Section {
        title: title.to_string(),
        rows,
        feature: true,
    };
    vec![
        feature(
            s.s_amcp,
            vec![
                row("claude / codex", s.f_detect),
                row(h.fk_launch, h.fd_launch),
                row(h.fk_bench, h.fd_bench),
                row("NEEDS ME", h.fd_needs),
                row(format!("\u{f06a9} {}", h.fk_floor), h.fd_floor),
                row(h.fk_watch, h.fd_watch),
                row(h.fk_access, h.fd_access),
                row(h.fk_rest, h.fd_rest),
                row(s.kf_restore, s.f_restore),
                row(h.fk_usage, h.fd_usage),
                row(h.fk_git, h.fd_git),
            ],
        ),
        feature(
            h.fs_reading,
            vec![
                row(h.fk_handover, h.fd_handover),
                row(h.fk_rail, h.fd_rail),
                row(h.fk_frame, h.fd_frame),
                row(h.fk_briefs, h.fd_briefs),
                row(format!("👓 {}", s.focus_name), s.f_focus),
                row(h.fk_pictures, h.fd_pictures),
                row(h.fk_sticky, h.fd_sticky),
                row(h.fk_comments, h.fd_comments),
            ],
        ),
        feature(
            s.s_feat,
            vec![
                row(s.kf_tiling, s.f_tiling),
                row(s.kf_groups, s.f_groups),
                row(h.fk_leftbar, h.fd_leftbar),
                row(s.kf_drag, s.f_drag),
                row(s.kf_rename, s.f_rename),
                row(s.kf_popout, s.f_popout),
                row(h.fk_marks, h.fd_marks),
                row(h.fk_trail, h.fd_trail),
                row(h.fk_lookup, h.fd_lookup),
                row(h.fk_scrollmode, h.fd_scrollmode),
                row(h.fk_sessions, h.fd_sessions),
            ],
        ),
        feature(
            s.s_theming,
            vec![
                row(h.fk_themes, h.fd_themes),
                row(s.kf_perpane, s.f_perpane),
                row(s.kf_grade, s.f_grade),
                row(h.fk_skins, h.fd_skins),
                row("UI CONFIG", h.fd_uiconfig),
            ],
        ),
        feature(
            s.s_crt,
            vec![
                row(s.kf_warp, s.f_warp),
                row(s.kf_scan, s.f_scan),
                row(s.kf_crawl, s.f_crawl),
            ],
        ),
        feature(
            h.fs_system,
            vec![
                row(format!("🤖 {}", s.kf_mcp), h.fd_mcp),
                row(h.fk_restricted, h.fd_restricted),
                row("terminal-delight ctl", h.fd_ctl),
                row(h.fk_updates, h.fd_updates),
                row(format!("🌐 {}", h.fk_langs), h.fd_langs),
                row(format!("🐞 {}", h.fk_bug), h.fd_bug),
                row("Omarchy", h.fd_omarchy),
            ],
        ),
    ]
}

/// How well one typed word is found in `hay`, already lowercased. `None` when
/// it is not there at all.
///
/// A substring scores highest, more at the start of a word. Failing that, the
/// word's letters in order inside ONE word of `hay`, within a stretch no
/// longer than twice the typed word, scored by how tight that stretch is
/// (`scrl` finds "scroll"). Letters gathered across a space are noise: "peel"
/// would otherwise find "Backs*p*ac*e* D*el*ete". Every start is tried, because
/// the first place the opening letter appears is rarely the tightest match.
fn word_score(hay: &[char], word: &[char]) -> Option<i64> {
    if word.is_empty() {
        return Some(0);
    }
    if let Some(at) = hay.windows(word.len()).position(|w| w == word) {
        let starts_word = at == 0 || !hay[at - 1].is_alphanumeric();
        return Some(if starts_word { 100 } else { 70 });
    }
    let limit = word.len() * 2;
    let mut best: Option<usize> = None;
    for start in (0..hay.len()).filter(|&i| hay[i] == word[0]) {
        let mut wi = 1;
        let mut end = start;
        for (i, &c) in hay.iter().enumerate().skip(start + 1) {
            if wi == word.len() || i - start >= limit || c.is_whitespace() {
                break;
            }
            if c == word[wi] {
                wi += 1;
                end = i;
            }
        }
        if wi == word.len() {
            let span = end - start + 1;
            best = Some(best.map_or(span, |b| b.min(span)));
        }
    }
    best.map(|span| 40 - (span - word.len()) as i64 * 2)
}

/// How well a row answers the whole query: every word must be found in the
/// row's key, its description or its section's title. A word found in the row
/// itself outranks the same word found only in the title, so typing a
/// section's name lists that section with its rows in their own order.
fn row_score(title: &str, row: &Row, words: &[Vec<char>]) -> Option<i64> {
    let own: Vec<char> = format!("{} {}", row.key, row.desc)
        .to_lowercase()
        .chars()
        .collect();
    let head: Vec<char> = title.to_lowercase().chars().collect();
    let mut total = 0;
    for w in words {
        let mine = word_score(&own, w);
        let theirs = word_score(&head, w).map(|s| s / 2);
        total += mine.max(theirs)?;
    }
    Some(total)
}

/// The sections that answer `query`, best first, each holding only its rows
/// that answer, best first. An empty or blank query returns `sections`
/// unchanged and in order.
pub fn filter(sections: &[Section], query: &str) -> Vec<Section> {
    let words: Vec<Vec<char>> = query
        .split_whitespace()
        .map(|w| w.to_lowercase().chars().collect())
        .collect();
    if words.is_empty() {
        return sections.to_vec();
    }
    let mut kept: Vec<(i64, Section)> = sections
        .iter()
        .filter_map(|sec| {
            let mut rows: Vec<(i64, &Row)> = sec
                .rows
                .iter()
                .filter_map(|r| row_score(&sec.title, r, &words).map(|s| (s, r)))
                .collect();
            // stable: equal scores keep the sheet's own order
            rows.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
            let best = rows.first()?.0;
            Some((
                best,
                Section {
                    title: sec.title.clone(),
                    rows: rows.into_iter().map(|(_, r)| r.clone()).collect(),
                    feature: sec.feature,
                },
            ))
        })
        .collect();
    kept.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
    kept.into_iter().map(|(_, s)| s).collect()
}

/// Cut `sections` into a left and a right column without reordering them,
/// at the point that leaves the two heights closest. A section costs its rows
/// plus two for its title and the gap under it.
pub fn columns(sections: Vec<Section>) -> (Vec<Section>, Vec<Section>) {
    let cost = |s: &Section| s.rows.len() + 2;
    let total: usize = sections.iter().map(cost).sum();
    let mut best = (usize::MAX, 0);
    let mut left = 0;
    for cut in 0..=sections.len() {
        let diff = left.max(total - left) - left.min(total - left);
        if diff < best.0 {
            best = (diff, cut);
        }
        if let Some(s) = sections.get(cut) {
            left += cost(s);
        }
    }
    let mut a = sections;
    let b = a.split_off(best.1);
    (a, b)
}

/// Rows across every section, for the count beside the filter.
pub fn count(sections: &[Section]) -> usize {
    sections.iter().map(|s| s.rows.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;

    fn sheet() -> Vec<Section> {
        let s = Lang::En.strings();
        let mut all = shortcuts(s);
        all.extend(features(s));
        all
    }

    fn keys(sections: &[Section]) -> Vec<String> {
        sections
            .iter()
            .flat_map(|s| s.rows.iter().map(|r| r.key.clone()))
            .collect()
    }

    #[test]
    fn an_empty_query_is_the_whole_sheet_in_order() {
        let all = sheet();
        assert_eq!(filter(&all, ""), all);
        assert_eq!(filter(&all, "   "), all);
    }

    #[test]
    fn a_word_narrows_to_the_rows_that_say_it() {
        // The exact word ranks first; "top-level" holds p…e…e…l in one word
        // and may follow it, which is what fuzzy means.
        let got = filter(&sheet(), "peel");
        assert_eq!(
            keys(&got).first().map(String::as_str),
            Some("Alt+Backspace")
        );
        assert!(count(&got) <= 3, "{:?}", keys(&got));
        // The chord's own line says "Stick a note", so the feature's line
        // carries the chord: whoever types the feature's name finds its key.
        let got = filter(&sheet(), "sticky");
        assert!(
            got.iter()
                .flat_map(|s| &s.rows)
                .any(|r| r.desc.contains("Alt+S")),
            "{got:?}"
        );
        assert!(!keys(&got).contains(&"Ctrl+Shift+T".to_string()));
        assert!(count(&got) < 8, "a filter that keeps {} rows", count(&got));
    }

    #[test]
    fn a_section_title_lists_that_section() {
        // "scroll mode" is a section's name; its rows say "a line", "search
        // this pane" and never the words themselves.
        let got = filter(&sheet(), "scroll mode");
        let sm = got
            .iter()
            .find(|s| s.title.starts_with("IN SCROLL MODE"))
            .expect("the scroll-mode section answers its own name");
        assert_eq!(sm.rows.len(), 7, "every row of the section, not a few");
        assert!(keys(&got).contains(&"q · Esc".to_string()));
    }

    #[test]
    fn every_word_must_be_found() {
        let got = filter(&sheet(), "note send");
        assert!(keys(&got).contains(&"Ctrl+Shift+Enter".to_string()));
        assert!(
            !keys(&got).contains(&"Alt+S".to_string()),
            "{:?}",
            keys(&got)
        );
    }

    #[test]
    fn letters_in_order_match_only_within_a_short_stretch() {
        let w: Vec<char> = "mrk".chars().collect();
        assert!(word_score(&"pane marks".chars().collect::<Vec<_>>(), &w).is_some());
        // m … r … k spread across a sentence is not a match
        let loose: Vec<char> = "move the cursor to the back".chars().collect();
        assert_eq!(word_score(&loose, &w), None);
    }

    #[test]
    fn the_tightest_start_is_the_one_scored() {
        // the first `t` is a word away from any `b`; the second starts "tab"
        let hay: Vec<char> = "the quick tab".chars().collect();
        let w: Vec<char> = "tb".chars().collect();
        assert!(word_score(&hay, &w).is_some());
        assert!(word_score(
            &"scroll mode".chars().collect::<Vec<_>>(),
            &"scrl".chars().collect::<Vec<_>>()
        )
        .is_some());
    }

    #[test]
    fn a_query_finds_shortcuts_and_features_together() {
        let got = filter(&sheet(), "marks");
        assert!(got.iter().any(|s| s.feature), "the feature tour answered");
        assert!(got.iter().any(|s| !s.feature), "the shortcuts answered");
    }

    #[test]
    fn nothing_matches_is_empty_not_everything() {
        assert!(filter(&sheet(), "zzzqqq").is_empty());
    }

    #[test]
    fn the_columns_keep_order_and_balance() {
        let all = shortcuts(Lang::En.strings());
        let (a, b) = columns(all.clone());
        let mut back = a.clone();
        back.extend(b.clone());
        assert_eq!(back, all, "cut, never reordered");
        let h = |v: &[Section]| v.iter().map(|s| s.rows.len() + 2).sum::<usize>();
        let biggest = all.iter().map(|s| s.rows.len() + 2).max().unwrap();
        assert!(h(&a).abs_diff(h(&b)) <= biggest, "{} vs {}", h(&a), h(&b));
    }

    #[test]
    fn the_columns_of_nothing_are_empty() {
        let (a, b) = columns(vec![]);
        assert!(a.is_empty() && b.is_empty());
    }

    /// Hands on Keyboard landed these chords on 2026-11-03 (PRs 1285 and
    /// 1287), and the sheet had five of them. A chord that only the
    /// changelog knows is a chord nobody finds.
    #[test]
    fn every_hands_on_keyboard_chord_is_on_the_sheet() {
        let ks = keys(&shortcuts(Lang::En.strings()));
        for k in [
            "Alt+O",
            "Alt+Q → 1 … 9",
            "Alt+1 … 9",
            "Alt+0 / Alt+Shift+0",
            "Shift+PgUp / PgDn",
            "Alt+E / Alt+Shift+E",
            "Alt+L",
            "Alt+[",
            "j / k · u / d",
            "{ / }",
            "Space / Shift+Space",
            "Tab / Shift+Tab",
            "Ctrl+Shift+Enter",
            "Alt+Shift+A … Z",
        ] {
            assert!(ks.contains(&k.to_string()), "{k} missing from the sheet");
        }
    }

    /// The reading keys on the sheet are the ones `keylayer::read_key`
    /// answers: a row printed for a key the document ignores is a lie.
    #[test]
    fn the_reading_rows_name_keys_the_document_takes() {
        use crate::keylayer::read_key;
        for (key, shift) in [
            ("j", false),
            ("k", false),
            ("space", false),
            ("space", true),
            ("g", false),
            ("g", true),
            ("tab", false),
            ("tab", true),
            ("]", false),
            ("[", false),
            ("enter", false),
            ("c", false),
            ("y", false),
            ("+", false),
            ("-", false),
            ("0", false),
            ("1", false),
            ("escape", false),
        ] {
            assert!(read_key(key, false, shift, false).is_some(), "{key}");
        }
        assert!(read_key("enter", true, true, false).is_some(), "the send");
    }

    /// Likewise scroll mode's rows against `navkeys::scroll_motion`.
    #[test]
    fn the_scroll_mode_rows_name_keys_the_mode_takes() {
        use crate::navkeys::scroll_motion;
        for (key, shift) in [
            ("j", false),
            ("k", false),
            ("u", false),
            ("d", false),
            ("g", false),
            ("G", false),
            ("[", false),
            ("]", false),
            ("{", false),
            ("}", false),
            ("e", false),
            ("E", false),
            ("/", false),
            ("q", false),
            ("escape", false),
        ] {
            assert!(scroll_motion(key, shift).is_some(), "{key}");
        }
    }

    #[test]
    fn every_language_builds_the_same_shape() {
        let en = sheet();
        for l in Lang::ALL {
            let s = l.strings();
            let mut all = shortcuts(s);
            all.extend(features(s));
            assert_eq!(all.len(), en.len(), "{}", l.code());
            for (a, b) in all.iter().zip(&en) {
                assert_eq!(a.rows.len(), b.rows.len(), "{} {}", l.code(), a.title);
                assert!(a.rows.iter().all(|r| !r.desc.is_empty()));
            }
        }
    }
}
