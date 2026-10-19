//! Alt+L — LOOKUP: every name in the window, in one list, found by typing part
//! of it.
//!
//! A window with twenty panes is found by eye: the sticky note first, then the
//! pane's title, then which tab it sits in under which branch of the left bar.
//! Each of those is a name somebody gave the thing or can read off it, and each
//! of them already had a place to look — the tree, the header, the paper on the
//! glass — but no place to TYPE. Ctrl+Shift+F searches what panes have printed;
//! this searches what they are called.
//!
//! Five kinds of row, and the kind is the tie-break: two rows that match equally
//! well rank the wider thing first, so typing a project's name lands on the
//! project rather than on the first tab that happens to share a word with it.
//!
//! Free of gpui and of `Workspace`, like [`crate::tree`]: the workspace hands
//! over the rows in left-bar order and gets back a ranking, so the order and
//! the matching can be tested without a window.

/// What a row is.
///
/// Declared widest first, because the derived order is the tie-break in
/// [`rank`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Kind {
    Project,
    Group,
    Tab,
    Pane,
    Note,
}

impl Kind {
    /// The word in the row's left column, in the left bar's own vocabulary —
    /// "group" is what every menu calls the middle layer.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Project => "PROJECT",
            Kind::Group => "GROUP",
            Kind::Tab => "TAB",
            Kind::Pane => "PANE",
            Kind::Note => "NOTE",
        }
    }

    /// [`Self::label`] in `l`, for the row a person reads.
    #[allow(dead_code)] // drawn by main.rs (the lookup row's kind column, ~15351) once it adopts this
    pub fn label_in(self, l: crate::lang::Lang) -> &'static str {
        let t = &l.strings().parts;
        match self {
            Kind::Project => t.lookup_kind_project,
            Kind::Group => t.lookup_kind_group,
            Kind::Tab => t.lookup_kind_tab,
            Kind::Pane => t.lookup_kind_pane,
            Kind::Note => t.lookup_kind_note,
        }
    }
}

/// Where a row goes when it is chosen.
///
/// `P` is a pane's identity: a gpui `EntityId` in the window, a plain number in
/// the tests. A pane carries the tab it was found in, but the jump re-finds the
/// tab from the pane, because tabs are numbered by position and a tab closing
/// while the panel is up moves every number after it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target<P> {
    Project(u32),
    Group(u32),
    Tab(usize),
    Pane(P),
}

/// One row, before any query.
#[derive(Clone, Debug)]
pub struct Entry<P> {
    pub kind: Kind,
    /// The name, exactly as the person reads it elsewhere: the left bar's
    /// label, the pane header's title, the words on the note. The only text
    /// that is matched.
    pub text: String,
    /// Where it sits, drawn under the name so that twenty panes all titled
    /// `✳ Claude Code` can be told apart. Never matched: a query for a project
    /// would otherwise light every tab and pane under it.
    pub place: String,
    pub target: Target<P>,
}

/// One ranked row: which entry, and the characters of its `text` that matched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub entry: usize,
    pub positions: Vec<usize>,
}

/// Rank `entries` against `query`.
///
/// An empty query is the whole index in the order given, which is the left
/// bar's — so Alt+L with nothing typed reads as the tree laid flat, with every
/// fold opened and every pane and note hung under its tab. Otherwise the rows
/// whose `text` fuzzy-matches, best first; equal scores go to the wider kind,
/// then to the earlier row.
pub fn rank<P>(entries: &[Entry<P>], query: &str) -> Vec<Hit> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return (0..entries.len())
            .map(|entry| Hit {
                entry,
                positions: vec![],
            })
            .collect();
    }
    let mut scored: Vec<(i64, Kind, usize, Vec<usize>)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            crate::pane::fuzzy_match(&e.text, &needle).map(|(s, pos)| (s, e.kind, i, pos))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    scored
        .into_iter()
        .map(|(_, _, entry, positions)| Hit { entry, positions })
        .collect()
}

/// A note's words as one line: the headline an agent gave it, then the text,
/// with every run of whitespace — the note's own line breaks included — folded
/// to one space. `None` for a note with nothing written on it, which has no
/// name to find.
pub fn note_line(title: Option<&str>, text: &str) -> Option<String> {
    let joined = title
        .into_iter()
        .chain(std::iter::once(text))
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ");
    (!joined.is_empty()).then_some(joined)
}

/// The line under a row: its ancestors, outermost first, joined the way the
/// shut strip joins a project to its groups, then one detail about the row
/// itself after a dot — how many tabs a branch holds, what a pane is running.
///
/// Empty parts are dropped, so a tab filed nowhere reads `2 panes` rather than
/// a row of separators with a count hanging off the end.
pub fn place(ancestors: &[&str], detail: &str) -> String {
    let crumbs = ancestors
        .iter()
        .filter(|p| !p.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" \u{203a} ");
    [crumbs.as_str(), detail]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
}

/// `n` of a thing, in words: `1 tab`, `3 tabs`.
pub fn count(n: usize, thing: &str) -> String {
    format!("{n} {thing}{}", if n == 1 { "" } else { "s" })
}

/// The things [`count_in`] counts — the three nouns the lookup's rows say.
#[allow(dead_code)] // passed by main.rs (~15010, ~15040, ~15219) once it adopts count_in
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Noun {
    Tab,
    Pane,
    Name,
}

/// [`count`] in `l`, with the plural that language uses for `n` — English's
/// trailing `s` is one rule among several, so each noun is a whole phrase.
#[allow(dead_code)] // drawn by main.rs (~15010, ~15040, ~15219) once it adopts this
pub fn count_in(l: crate::lang::Lang, n: usize, noun: Noun) -> String {
    let t = &l.strings().parts;
    let p = match noun {
        Noun::Tab => &t.lookup_count_tab,
        Noun::Pane => &t.lookup_count_pane,
        Noun::Name => &t.lookup_count_name,
    };
    l.count(p, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(kind: Kind, text: &str, target: Target<u32>) -> Entry<u32> {
        Entry {
            kind,
            text: text.to_string(),
            place: String::new(),
            target,
        }
    }

    /// Parker's own window, cut down: a project, a group in it, a tab in the
    /// group, an agent pane in the tab and the note stuck on it.
    fn window() -> Vec<Entry<u32>> {
        vec![
            e(Kind::Project, "TERMINAL DELIGHT", Target::Project(1)),
            e(Kind::Group, "gliff initiative", Target::Group(7)),
            e(Kind::Tab, "Lookup", Target::Tab(3)),
            e(Kind::Pane, "\u{2733} Claude Code", Target::Pane(42)),
            e(Kind::Note, "go here.", Target::Pane(42)),
            e(Kind::Tab, "THREAT!", Target::Tab(4)),
        ]
    }

    #[test]
    fn nothing_typed_is_the_whole_tree_in_its_own_order() {
        let hits = rank(&window(), "   ");
        assert_eq!(
            hits.iter().map(|h| h.entry).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4, 5]
        );
        assert!(hits.iter().all(|h| h.positions.is_empty()));
    }

    #[test]
    fn a_note_is_found_by_its_words_and_lands_on_its_pane() {
        let w = window();
        let hits = rank(&w, "go he");
        let top = &w[hits[0].entry];
        assert_eq!(top.kind, Kind::Note);
        assert_eq!(top.target, Target::Pane(42));
        assert_eq!(hits[0].positions, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn every_kind_is_reachable_by_its_own_name() {
        let w = window();
        for (query, kind) in [
            ("terminal", Kind::Project),
            ("gliff", Kind::Group),
            ("lookup", Kind::Tab),
            ("claude", Kind::Pane),
            ("threat", Kind::Tab),
        ] {
            let hits = rank(&w, query);
            assert_eq!(w[hits[0].entry].kind, kind, "{query}");
        }
    }

    /// Case never matters: the left bar draws project names in capitals that
    /// were typed in lower case, and a person types what they see either way.
    #[test]
    fn matching_ignores_case_in_both_directions() {
        let w = window();
        assert_eq!(w[rank(&w, "LOOKUP")[0].entry].kind, Kind::Tab);
        assert_eq!(w[rank(&w, "delight")[0].entry].kind, Kind::Project);
    }

    #[test]
    fn a_tie_goes_to_the_wider_thing() {
        let w = vec![
            e(Kind::Pane, "jev", Target::Pane(1)),
            e(Kind::Tab, "jev", Target::Tab(0)),
            e(Kind::Project, "jev", Target::Project(2)),
            e(Kind::Group, "jev", Target::Group(3)),
        ];
        let kinds: Vec<Kind> = rank(&w, "jev").iter().map(|h| w[h.entry].kind).collect();
        assert_eq!(
            kinds,
            vec![Kind::Project, Kind::Group, Kind::Tab, Kind::Pane]
        );
    }

    #[test]
    fn a_better_match_beats_a_wider_kind() {
        let w = vec![
            e(Kind::Project, "HCVIDEO", Target::Project(1)),
            e(Kind::Tab, "VIDEO FINDER", Target::Tab(0)),
        ];
        // Both say "video" in one run, but only the tab says it at the start
        // of a word, so the tab leads despite being the narrower kind.
        let hits = rank(&w, "video");
        assert_eq!(hits.len(), 2);
        assert_eq!(w[hits[0].entry].kind, Kind::Tab);
    }

    #[test]
    fn the_place_is_drawn_and_never_matched() {
        let mut w = window();
        w[2].place = place(&["TERMINAL DELIGHT", "gliff initiative"], "1 pane");
        let hits = rank(&w, "terminal");
        assert_eq!(hits.len(), 1, "only the project's own name says terminal");
        assert_eq!(w[hits[0].entry].kind, Kind::Project);
    }

    #[test]
    fn no_match_is_an_empty_list() {
        assert!(rank(&window(), "zzzz").is_empty());
    }

    #[test]
    fn a_note_reads_as_one_line_with_its_headline_first() {
        assert_eq!(
            note_line(Some("GET MILK!"), "before\n  six\tpm"),
            Some("GET MILK! before six pm".to_string())
        );
        assert_eq!(note_line(None, "go here."), Some("go here.".to_string()));
        assert_eq!(note_line(None, " \n "), None, "a blank note has no name");
        assert_eq!(note_line(Some("  "), ""), None);
    }

    #[test]
    fn the_place_line_skips_what_is_missing() {
        assert_eq!(
            place(&["JOB", "", "sybil"], "CLAUDE"),
            "JOB \u{203a} sybil \u{b7} CLAUDE"
        );
        assert_eq!(place(&["JOB"], ""), "JOB");
        assert_eq!(place(&["", ""], "2 panes"), "2 panes");
        assert_eq!(place(&[], ""), "");
    }

    #[test]
    fn a_count_says_its_noun_in_the_right_number() {
        assert_eq!(count(0, "tab"), "0 tabs");
        assert_eq!(count(1, "tab"), "1 tab");
        assert_eq!(count(4, "pane"), "4 panes");
    }

    /// The translated count says exactly what the English one does in English,
    /// and the kind column likewise.
    #[test]
    fn a_count_in_a_language_matches_the_english_in_english() {
        use crate::lang::Lang;
        for n in [0, 1, 2, 5] {
            assert_eq!(count_in(Lang::En, n, Noun::Tab), count(n, "tab"));
            assert_eq!(count_in(Lang::En, n, Noun::Pane), count(n, "pane"));
            assert_eq!(count_in(Lang::En, n, Noun::Name), count(n, "name"));
        }
        for k in [
            Kind::Project,
            Kind::Group,
            Kind::Tab,
            Kind::Pane,
            Kind::Note,
        ] {
            assert_eq!(k.label_in(Lang::En), k.label());
        }
    }
}
