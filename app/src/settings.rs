//! Settings: one book for every setting the window has, laid out the way
//! T3 Code lays out its own.
//!
//! Before this the settings were scattered across a dozen floating panels,
//! each opened from its own glyph: rest and sleep in 🤖, the agent wall's
//! switches on the Threshing Floor, notices in 🔔, plugins in 🧩, the language
//! in a picker of its own. Parker, 2026-11-09: *"SETTINGS being the sort of
//! floating mess that it is right now will get competely overhauldd … an
//! ENTIRE MONOLITH like any moder app using T3 as a design standard now …
//! the section heading and names and glyphs will reflect what T3 code does."*
//!
//! This file is the pure half: which pages there are, the groups the nav
//! shows them in, each page's glyph, and the search. The names, groups and
//! order follow T3's `SETTINGS_NAV_GROUPS` (pingdotgg/t3code, MIT,
//! `apps/web/src/components/settings/settingsSearch.ts`), keeping only the
//! pages Terminal Delight has something to put on. The glyphs are the nearest
//! single characters to T3's lucide icons. The view is `settings_view.rs`;
//! the words are `lang/settings.rs`.

/// One page of the book.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    General,
    Appearance,
    Notifications,
    Keybindings,
    Providers,
    Threads,
    Projects,
    Worktrees,
    Environments,
    Access,
    Maintenance,
    Labs,
    About,
    Archive,
}

/// A heading over a run of pages in the nav. T3 names three and sets the
/// first and last runs apart by space alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heading {
    Agents,
    Code,
    Connections,
}

/// The nav, top to bottom.
pub const GROUPS: &[(Option<Heading>, &[Page])] = &[
    (
        None,
        &[
            Page::General,
            Page::Appearance,
            Page::Notifications,
            Page::Keybindings,
        ],
    ),
    (Some(Heading::Agents), &[Page::Providers, Page::Threads]),
    (Some(Heading::Code), &[Page::Projects, Page::Worktrees]),
    (
        Some(Heading::Connections),
        &[Page::Environments, Page::Access],
    ),
    (
        None,
        &[Page::Maintenance, Page::Labs, Page::About, Page::Archive],
    ),
];

impl Page {
    /// Every page, in nav order.
    pub fn all() -> impl Iterator<Item = Page> {
        GROUPS.iter().flat_map(|(_, pages)| pages.iter().copied())
    }

    /// The page's name in a scene fixture and on the command line.
    pub fn id(self) -> &'static str {
        match self {
            Page::General => "general",
            Page::Appearance => "appearance",
            Page::Notifications => "notifications",
            Page::Keybindings => "keybindings",
            Page::Providers => "providers",
            Page::Threads => "threads",
            Page::Projects => "projects",
            Page::Worktrees => "worktrees",
            Page::Environments => "environments",
            Page::Access => "access",
            Page::Maintenance => "maintenance",
            Page::Labs => "labs",
            Page::About => "about",
            Page::Archive => "archive",
        }
    }

    pub fn from_id(id: &str) -> Option<Page> {
        Page::all().find(|p| p.id() == id)
    }

    /// The nav glyph: the nearest single character to the lucide icon T3
    /// gives the same page (Settings2, Palette, Bell, Keyboard, Bot,
    /// MessagesSquare, PanelsTopLeft, FolderTree, Server, ShieldCheck, Wrench,
    /// FlaskConical, Info, Archive).
    pub fn glyph(self) -> &'static str {
        match self {
            Page::General => "\u{2699}",        // ⚙
            Page::Appearance => "\u{1f3a8}",    // 🎨
            Page::Notifications => "\u{1f514}", // 🔔
            Page::Keybindings => "\u{2328}",    // ⌨
            Page::Providers => "\u{1f916}",     // 🤖
            Page::Threads => "\u{1f4ac}",       // 💬
            Page::Projects => "\u{25a6}",       // ▦
            Page::Worktrees => "\u{1f5c2}",     // 🗂
            Page::Environments => "\u{1f5a7}",  // 🖧
            Page::Access => "\u{1f6e1}",        // 🛡
            Page::Maintenance => "\u{1f527}",   // 🔧
            Page::Labs => "\u{2697}",           // ⚗
            Page::About => "\u{24d8}",          // ⓘ
            Page::Archive => "\u{1f5c4}",       // 🗄
        }
    }

    /// The next or previous page in nav order, stopping at the ends.
    pub fn step(self, forward: bool) -> Page {
        let all: Vec<Page> = Page::all().collect();
        let i = all.iter().position(|p| *p == self).unwrap_or(0);
        let j = if forward {
            (i + 1).min(all.len() - 1)
        } else {
            i.saturating_sub(1)
        };
        all[j]
    }
}

/// One thing the search can find: a row, or a page by its name.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub page: Page,
    /// The row's title, or the page's name for the page itself.
    pub title: String,
    /// Everything else the row is known by: its plain line, its section, the
    /// page's name, and words a person might type for it.
    pub terms: Vec<String>,
    /// A page result, as against a row on it. Ranked after the rows, as T3
    /// ranks its secondary results.
    pub secondary: bool,
}

fn norm(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The items matching `query`, best first, as T3's `searchSettings` ranks
/// them: every word of the query must appear in the title or a term; then a
/// title that IS the query, starts with it, contains it, contains every word,
/// and last one that only a term holds. Rows before pages; ties keep the
/// order the items were given in. An empty query finds nothing.
pub fn search(query: &str, items: &[Item]) -> Vec<usize> {
    let q = norm(query);
    if q.is_empty() {
        return Vec::new();
    }
    let words: Vec<&str> = q.split(' ').collect();
    let mut hits: Vec<(bool, u8, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let title = norm(&item.title);
            let mut fields = vec![title.clone()];
            fields.extend(item.terms.iter().map(|t| norm(t)));
            if !words.iter().all(|w| fields.iter().any(|f| f.contains(w))) {
                return None;
            }
            let rank = if title == q {
                5
            } else if title.starts_with(&q) {
                4
            } else if title.contains(&q) {
                3
            } else if words.iter().all(|w| title.contains(w)) {
                2
            } else if fields.iter().any(|f| f.contains(&q)) {
                1
            } else {
                0
            };
            Some((item.secondary, rank, i))
        })
        .collect();
    hits.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    hits.into_iter().map(|(_, _, i)| i).collect()
}

/// A panel that still has its own drawing, shown on the book's right page
/// in place of the page's rows rather than over the window. Parker,
/// 2026-11-09: *"Open SHOULD open it in place! not LCOSE seetinga and open
/// the fconfig being explored"*.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    /// The surface config: theme, style, rules, UI CONFIG.
    Palette,
    Gauges,
    Scale,
    Language,
    Updates,
    Admin,
    Notices,
    Keys,
    Skills,
    Savings,
    Bug,
    Plugins,
    Recover,
}

/// The book's state while it is open.
#[derive(Clone, Debug, PartialEq)]
pub struct Book {
    pub page: Page,
    /// The panel shown in place on this page, while it is open.
    pub host: Option<Host>,
    /// The search box's text.
    pub query: String,
    /// The lit result while searching.
    pub pick: usize,
}

impl Book {
    pub fn at(page: Page) -> Book {
        Book {
            page,
            host: None,
            query: String::new(),
            pick: 0,
        }
    }

    pub fn searching(&self) -> bool {
        !self.query.trim().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nav_lists_every_page_once_under_t3s_headings() {
        let all: Vec<Page> = Page::all().collect();
        let mut ids: Vec<&str> = all.iter().map(|p| p.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "a page appears twice");
        assert_eq!(all.first(), Some(&Page::General), "General leads, as in T3");
        let headings: Vec<_> = GROUPS.iter().map(|(h, _)| *h).collect();
        assert_eq!(
            headings,
            [
                None,
                Some(Heading::Agents),
                Some(Heading::Code),
                Some(Heading::Connections),
                None
            ]
        );
        for p in &all {
            assert_eq!(Page::from_id(p.id()), Some(*p));
            assert!(!p.glyph().is_empty());
        }
        assert_eq!(Page::General.step(false), Page::General, "stops at the top");
        assert_eq!(Page::Archive.step(true), Page::Archive, "and at the bottom");
        assert_eq!(
            Page::Keybindings.step(true),
            Page::Providers,
            "across a heading"
        );
    }

    fn item(page: Page, title: &str, terms: &[&str], secondary: bool) -> Item {
        Item {
            page,
            title: title.into(),
            terms: terms.iter().map(|t| t.to_string()).collect(),
            secondary,
        }
    }

    /// T3's order: an exact title, then a title that starts with the query,
    /// then one that contains it, then one with every word, then a match only
    /// in the terms; rows before pages; and every word must be found
    /// somewhere or the item is out.
    ///
    /// Mutation-tested: dropping the every-word filter lets "sleep zzz"
    /// through; ranking pages with rows puts the Threads page first.
    #[test]
    fn search_ranks_as_t3_does() {
        let items = vec![
            item(Page::Threads, "Threads", &["rest", "sleep"], true),
            item(Page::Threads, "Sleep after resting", &["sleep"], false),
            item(Page::Threads, "Sleep resting agents", &["frozen"], false),
            item(Page::Threads, "Rest idle agents", &["frozen sleep"], false),
            item(Page::General, "Sleep", &[], false),
            item(Page::Access, "Agent access", &["mcp server"], false),
        ];
        assert_eq!(search("sleep", &items), [4, 1, 2, 3, 0]);
        assert_eq!(search("  ", &items), Vec::<usize>::new());
        assert_eq!(search("sleep zzz", &items), Vec::<usize>::new());
        assert_eq!(search("MCP", &items), [5], "terms count, case aside");
        assert_eq!(
            search("agents rest", &items),
            [2, 3],
            "every word in the title, ties in order"
        );
    }
}
