//! Settings, drawn: a book over the window, the nav on the left page and the
//! setting on the right, in the anatomy T3 Code gives its own settings.
//!
//! The model (pages, groups, glyphs, search) is `settings.rs`; the words are
//! `lang/settings.rs`. This file decides how a page looks and what each
//! control does. A control here calls the same function the old panel's
//! control called, so a setting has one implementation whichever way a person
//! reaches it. Rows whose setting still lives in a panel of its own carry an
//! Open button that closes the book and opens that panel, so nothing is lost
//! while the panels move in one by one (`docs/plans/settings-monolith/`).

use super::*;
use crate::settings::{self, Book, Heading, Host, Item, Page};
use crate::wtclean;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The Worktrees page's review: what the last look found, and whether the
/// remove button has been pressed once.
#[derive(Default)]
pub(crate) struct WtReview {
    pub(crate) busy: bool,
    /// Every worktree found, and how many repositories were looked in.
    pub(crate) got: Option<Result<(Vec<wtclean::Facts>, usize), String>>,
    /// The remove button has been pressed once; the next press removes.
    pub(crate) armed: bool,
    /// What the last removal did.
    pub(crate) said: Option<String>,
    pub(crate) task: Option<gpui::Task<()>>,
    /// Tests only: the directories and home to look in, instead of the
    /// window's panes and the person's own projects.
    pub(crate) fixture: Option<(Vec<PathBuf>, PathBuf)>,
}

/// Every repository the window's panes are in and every local project the
/// composer lists, each worktree in them read. Runs off the window's thread.
fn wt_gather(cwds: &[PathBuf], home: &Path) -> Result<(Vec<wtclean::Facts>, usize), String> {
    let mut dirs: Vec<PathBuf> = cwds
        .iter()
        .map(|c| std::fs::canonicalize(c).unwrap_or_else(|_| c.clone()))
        .collect();
    // Anything running in a worktree keeps it, not only this window's panes.
    let mut cwds = dirs.clone();
    cwds.extend(wtclean::process_cwds());
    dirs.extend(
        launcher::scan(&launcher::default_roots(home), 1000)
            .into_iter()
            .filter(|p| p.repo && p.machine.is_none())
            .map(|p| p.path),
    );
    let repos: BTreeSet<PathBuf> = dirs.iter().filter_map(|d| wtclean::repo_of(d)).collect();
    let cwds = cwds;
    let mut facts = Vec::new();
    for r in &repos {
        if let Ok(mut f) = wtclean::scan(r, &cwds) {
            facts.append(&mut f);
        }
    }
    Ok((facts, repos.len()))
}

/// What an Open button does once the book has closed.
type Then = fn(&mut Workspace, &mut Window, &mut Context<Workspace>);
/// What a switch does when pressed.
type Flip = fn(&mut Workspace, &mut Context<Workspace>);

/// Frost everything outside the element this sits in, for the frame: the
/// CRT post-pass blurs the window behind an overlay and leaves the overlay
/// sharp. Parker, 2026-11-09: *"BLUR the main surface while an over lay is
/// displayed."* Re-armed every frame it is drawn, so it lifts the frame after.
pub(crate) fn frost_behind() -> gpui::Div {
    div().absolute().inset_0().child(
        gpui::canvas(
            |bounds, window, _cx| {
                let sf = window.scale_factor();
                crate::warp::set_focus_blur(
                    [
                        f32::from(bounds.origin.x) * sf,
                        f32::from(bounds.origin.y) * sf,
                        f32::from(bounds.size.width) * sf,
                        f32::from(bounds.size.height) * sf,
                    ],
                    22.0 * sf,
                    12.0 * sf,
                    0.55,
                    10.0 * sf,
                );
            },
            |_, _, _, _| {},
        )
        .size_full(),
    )
}

/// The nav page's width, as T3's sidebar is narrower than its page.
const NAV_W: f32 = 232.;

impl Workspace {
    /// Open the book at `page`, or at the page it was last on.
    pub(crate) fn open_settings(&mut self, page: Option<Page>, cx: &mut Context<Self>) {
        let page = page.or(self.settings_last).unwrap_or(Page::General);
        self.settings = Some(Book::at(page));
        self.settings_last = Some(page);
        self.settings_landed(cx);
        cx.notify();
    }

    /// A page came on screen. The Worktrees page looks for worktrees the
    /// first time it shows, so the review is there without a press.
    fn settings_landed(&mut self, cx: &mut Context<Self>) {
        if self.settings.as_ref().map(|b| b.page) == Some(Page::Worktrees)
            && self.wt_review.got.is_none()
            && !self.wt_review.busy
        {
            self.wt_look(cx);
        }
    }

    fn wt_cwds(&self, cx: &App) -> Vec<PathBuf> {
        self.every_view()
            .iter()
            .filter_map(|v| v.read(cx).current_cwd().map(PathBuf::from))
            .collect()
    }

    /// Where to look: the panes' directories and home, or a test's.
    fn wt_where(&self, cx: &App) -> (Vec<PathBuf>, PathBuf) {
        match &self.wt_review.fixture {
            Some((cwds, home)) => (cwds.clone(), home.clone()),
            None => (self.wt_cwds(cx), session::home_dir()),
        }
    }

    fn wt_save(&mut self) {
        self.wt_review.armed = false;
        if let Err(e) = wtclean::save(&instance::config_dir(), &self.wt_prefs) {
            self.wt_review.said = Some(e);
        }
    }

    /// Look for worktrees, off the window's thread.
    pub(crate) fn wt_look(&mut self, cx: &mut Context<Self>) {
        if self.wt_review.busy {
            return;
        }
        let (cwds, home) = self.wt_where(cx);
        self.wt_review.busy = true;
        self.wt_review.armed = false;
        let task = cx.spawn(async move |this, cx| {
            let got = cx
                .background_spawn(async move { wt_gather(&cwds, &home) })
                .await;
            this.update(cx, |ws, cx| {
                ws.wt_review.busy = false;
                ws.wt_review.got = Some(got);
                cx.notify();
            })
            .ok();
        });
        self.wt_review.task = Some(task);
        cx.notify();
    }

    /// The candidates under the switches as they stand now.
    fn wt_doomed(&self) -> Vec<(PathBuf, PathBuf)> {
        let Some(Ok((facts, _))) = &self.wt_review.got else {
            return Vec::new();
        };
        let now = SystemTime::now();
        facts
            .iter()
            .filter(|f| wtclean::judge(f, &self.wt_prefs, now).removes())
            .map(|f| (f.repo.clone(), f.path.clone()))
            .collect()
    }

    /// The remove button: the first press arms it, the second removes every
    /// candidate with plain `git worktree remove` and looks again. A pane
    /// that moved into a candidate since the look keeps it.
    pub(crate) fn wt_remove_press(&mut self, cx: &mut Context<Self>) {
        let doomed = self.wt_doomed();
        if doomed.is_empty() || self.wt_review.busy {
            return;
        }
        if !self.wt_review.armed {
            self.wt_review.armed = true;
            cx.notify();
            return;
        }
        self.wt_review.armed = false;
        self.wt_review.busy = true;
        let (cwds, home) = self.wt_where(cx);
        let s = self.lang.strings().settings;
        let task = cx.spawn(async move |this, cx| {
            let (removed, refused, got) = cx
                .background_spawn(async move {
                    let mut canon: Vec<PathBuf> = cwds
                        .iter()
                        .map(|c| std::fs::canonicalize(c).unwrap_or_else(|_| c.clone()))
                        .collect();
                    canon.extend(wtclean::process_cwds());
                    let mut removed = 0usize;
                    let mut refused: Vec<String> = Vec::new();
                    for (repo, path) in doomed {
                        if canon.iter().any(|c| c.starts_with(&path)) {
                            continue;
                        }
                        match wtclean::remove(&repo, &path) {
                            Ok(()) => removed += 1,
                            Err(e) => refused.push(format!("{}: {e}", path.display())),
                        }
                    }
                    (removed, refused, wt_gather(&cwds, &home))
                })
                .await;
            this.update(cx, |ws, cx| {
                let mut said = lang::fill(s.wt_removed, &[("n", &removed.to_string())]);
                if let Some(first) = refused.first() {
                    said = format!(
                        "{said} \u{b7} {}",
                        lang::fill(
                            s.wt_failed,
                            &[("n", &refused.len().to_string()), ("first", first)]
                        )
                    );
                }
                ws.wt_review.busy = false;
                ws.wt_review.said = Some(said);
                ws.wt_review.got = Some(got);
                cx.notify();
            })
            .ok();
        });
        self.wt_review.task = Some(task);
        cx.notify();
    }

    /// Whether a hosted panel is still open, by its own flag.
    fn host_open(&self, h: Host) -> bool {
        match h {
            Host::Palette => self.theme_menu.is_some(),
            Host::Gauges => self.osd_menu.is_some(),
            Host::Scale => self.scale_menu,
            Host::Language => self.lang_picker.is_some(),
            Host::Updates => self.update_menu,
            Host::Admin => self.admin_menu,
            Host::Notices => self.notif_menu,
            Host::Keys => self.help_open,
            Host::Skills => self.agents_menu,
            Host::Savings => self.savings_menu,
            Host::Bug => self.bug_menu,
            Host::Plugins => self.plugins_menu,
            Host::Recover => self.dead_menu,
        }
    }

    /// The panel the book is showing in place, while it is open.
    pub(crate) fn hosting(&self) -> Option<Host> {
        self.settings
            .as_ref()
            .and_then(|b| b.host)
            .filter(|h| self.host_open(*h))
    }

    /// Close the panel shown in place and go back to the page. True when
    /// there was one.
    pub(crate) fn unhost(&mut self) -> bool {
        let Some(h) = self.hosting() else {
            if let Some(b) = self.settings.as_mut() {
                b.host = None;
            }
            return false;
        };
        match h {
            Host::Palette => {
                self.theme_menu = None;
                self.design_menu = None;
            }
            Host::Gauges => self.osd_menu = None,
            Host::Scale => self.scale_menu = false,
            Host::Language => self.lang_picker = None,
            Host::Updates => self.close_updates(),
            Host::Admin => self.close_admin(),
            Host::Notices => self.notif_menu = false,
            Host::Keys => self.help_open = false,
            Host::Skills => {
                self.close_skill_form();
                self.agents_menu = false;
            }
            Host::Savings => self.savings_menu = false,
            Host::Bug => self.close_bug_menu(),
            Host::Plugins => self.plugins_menu = false,
            Host::Recover => self.dead_menu = false,
        }
        if let Some(b) = self.settings.as_mut() {
            b.host = None;
        }
        true
    }

    /// A panel's drawing goes onto the book's page when the book is showing
    /// it in place, and onto the window otherwise.
    pub(crate) fn host_or_root<E: IntoElement>(
        &self,
        h: Host,
        el: Option<E>,
        slot: &mut Option<AnyElement>,
    ) -> Option<AnyElement> {
        let el = el.map(IntoElement::into_any_element);
        if self.hosting() == Some(h) {
            *slot = el;
            None
        } else {
            el
        }
    }

    /// The title of the row that opened `h`, for the breadcrumb.
    fn host_title(&self, h: Host) -> &'static str {
        let s = self.lang.strings().settings;
        match h {
            Host::Palette => s.r_palette,
            Host::Gauges => s.r_gauges,
            Host::Scale => s.r_scale,
            Host::Language => s.r_language,
            Host::Updates => s.r_updates,
            Host::Admin => s.r_admin,
            Host::Notices => s.r_notices,
            Host::Keys => s.r_keys,
            Host::Skills => s.r_skill_list,
            Host::Savings => s.r_savings,
            Host::Bug => s.r_bug,
            Host::Plugins => s.r_plugins,
            Host::Recover => s.r_recover,
        }
    }

    pub(crate) fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.unhost();
        if let Some(b) = self.settings.take() {
            self.settings_last = Some(b.page);
        }
        if self.agents_drag.take().is_some() {
            agentcfg::save(&self.agents_prefs);
        }
        cx.notify();
    }

    /// Esc without a context: a search is cleared, else the book is put
    /// away on the page it was on (`close_popups`).
    pub(crate) fn settings_peel(&mut self) {
        // A panel shown in place closes first, back to its page.
        if self.unhost() {
            return;
        }
        match self.settings.as_mut() {
            Some(b) if b.searching() => {
                b.query.clear();
                b.pick = 0;
            }
            Some(_) => {
                self.settings_last = self.settings.take().map(|b| b.page);
            }
            None => {}
        }
    }

    pub(crate) fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        if self.settings.is_some() {
            self.close_settings(cx);
        } else {
            self.open_settings(None, cx);
        }
    }

    fn settings_go(&mut self, page: Page, cx: &mut Context<Self>) {
        self.unhost();
        if let Some(b) = self.settings.as_mut() {
            b.page = page;
            b.query.clear();
            b.pick = 0;
        }
        self.settings_last = Some(page);
        self.settings_landed(cx);
        cx.notify();
    }

    /// Escape: a search is cleared before the book closes. True when the
    /// book took the key.
    pub(crate) fn settings_escape(&mut self, cx: &mut Context<Self>) -> bool {
        match self.settings.as_mut() {
            Some(b) if b.searching() => {
                b.query.clear();
                b.pick = 0;
                cx.notify();
                true
            }
            Some(_) => {
                if !self.unhost() {
                    self.close_settings(cx);
                }
                cx.notify();
                true
            }
            None => false,
        }
    }

    /// The book's keyboard while it is open: typing searches, the arrows walk
    /// the pages (or the results while searching), Enter opens the lit
    /// result. Every key is the book's, so a stray letter never reaches the
    /// shell behind it; chords with Ctrl, Alt or Super are left alone except
    /// Ctrl+Backspace, which clears the search.
    pub(crate) fn settings_key(&mut self, ks: &gpui::Keystroke, cx: &mut Context<Self>) -> bool {
        if self.settings.is_none() {
            return false;
        }
        // A panel shown in place keeps its own keys (the language search,
        // the keys sheet's filter); Esc reaches `close_popups`, which closes
        // it back to the page.
        if self.hosting().is_some() {
            return false;
        }
        let m = &ks.modifiers;
        let key = ks.key.as_str();
        if key == "escape" {
            return self.settings_escape(cx);
        }
        let hits = self.settings_hits();
        if key == "," && m.control {
            self.close_settings(cx);
            return true;
        }
        let Some(b) = self.settings.as_mut() else {
            return false;
        };
        match key {
            "backspace" if m.control => b.query.clear(),
            "backspace" => {
                b.query.pop();
                b.pick = 0;
            }
            "down" | "up" if b.searching() => {
                b.pick = if key == "down" {
                    (b.pick + 1).min(hits.len().saturating_sub(1))
                } else {
                    b.pick.saturating_sub(1)
                };
            }
            "down" | "up" | "tab" => {
                b.page = b.page.step(key != "up" && !(key == "tab" && m.shift));
                self.settings_last = Some(b.page);
            }
            "enter" if b.searching() => {
                if let Some(item) = hits.get(b.pick) {
                    b.page = item.page;
                    b.query.clear();
                    b.pick = 0;
                    self.settings_last = Some(item.page);
                }
            }
            _ if m.control || m.alt || m.platform || m.function => return false,
            _ => {
                if let Some(ch) = ks
                    .key_char
                    .as_deref()
                    .filter(|c| !c.chars().any(char::is_control))
                {
                    if b.query.chars().count() < 48 {
                        b.query.push_str(ch);
                        b.pick = 0;
                    }
                }
            }
        }
        self.settings_landed(cx);
        cx.notify();
        true
    }

    fn page_name(&self, p: Page) -> &'static str {
        page_name_in(self.lang, p)
    }

    fn heading_name(&self, h: Heading) -> &'static str {
        let s = self.lang.strings().settings;
        match h {
            Heading::Agents => s.h_agents,
            Heading::Code => s.h_code,
            Heading::Connections => s.h_connections,
        }
    }

    pub(crate) fn settings_items(&self) -> Vec<Item> {
        settings_items_in(self.lang)
    }

    fn settings_hits(&self) -> Vec<Item> {
        let Some(b) = self.settings.as_ref() else {
            return Vec::new();
        };
        let items = self.settings_items();
        settings::search(&b.query, &items)
            .into_iter()
            .map(|i| items[i].clone())
            .collect()
    }
}

/// A page's name in a language.
fn page_name_in(l: lang::Lang, p: Page) -> &'static str {
    let s = l.strings().settings;
    match p {
        Page::General => s.p_general,
        Page::Appearance => s.p_appearance,
        Page::Notifications => s.p_notifications,
        Page::Keybindings => s.p_keybindings,
        Page::Providers => s.p_providers,
        Page::Threads => s.p_threads,
        Page::Projects => s.p_projects,
        Page::Worktrees => s.p_worktrees,
        Page::Environments => s.p_environments,
        Page::Access => s.p_access,
        Page::Maintenance => s.p_maintenance,
        Page::Labs => s.p_labs,
        Page::About => s.p_about,
        Page::Archive => s.p_archive,
    }
}

/// Everything the search can find: each page by its name, and each row by
/// its title, with its plain line, its section and its page as terms.
/// Kept beside the pages that draw the rows; the test
/// `every_page_has_rows_the_search_can_find` holds the two together.
fn settings_items_in(l: lang::Lang) -> Vec<Item> {
    let s = l.strings().settings;
    let ui = l.strings().ui;
    let rows: &[(Page, &str, &str, &str)] = &[
        (Page::General, s.s_language, s.r_language, s.r_language_sub),
        (Page::General, s.s_window, s.r_scale, s.r_scale_sub),
        (Page::General, s.s_window, s.r_updates, s.r_updates_sub),
        (Page::General, s.s_window, s.r_admin, s.r_admin_sub),
        (Page::Appearance, s.s_look, s.r_palette, s.r_palette_sub),
        (Page::Appearance, s.s_look, s.r_gauges, s.r_gauges_sub),
        (Page::Appearance, s.s_agent_panes, s.r_tint, s.r_tint_sub),
        (
            Page::Notifications,
            s.s_notices,
            s.r_notices,
            s.r_notices_sub,
        ),
        (Page::Keybindings, s.s_keys, s.r_keys, s.r_keys_sub),
        (
            Page::Keybindings,
            s.s_keys,
            s.r_settings_key,
            s.r_settings_key_sub,
        ),
        (Page::Providers, s.s_skills, s.r_skills, s.r_skills_sub),
        (
            Page::Providers,
            s.s_skills,
            s.r_skill_list,
            s.r_skill_list_sub,
        ),
        (Page::Providers, s.s_launch, s.r_local, ""),
        (Page::Threads, s.s_rest, ui.ag_rest_name, ui.ag_rest_blurb),
        (Page::Threads, s.s_rest, ui.ag_rest_after, ""),
        (Page::Threads, s.s_rest, ui.ag_sleep_name, ui.ag_sleep_blurb),
        (Page::Threads, s.s_rest, ui.ag_sleep_after, ""),
        (
            Page::Projects,
            s.s_projects,
            s.r_add_project,
            s.r_add_project_sub,
        ),
        (Page::Projects, s.s_projects, s.r_projects_file, ""),
        (
            Page::Worktrees,
            s.s_cleanup,
            s.r_wt_merged,
            s.r_wt_merged_sub,
        ),
        (
            Page::Worktrees,
            s.s_cleanup,
            s.r_wt_inactive,
            s.r_wt_inactive_sub,
        ),
        (
            Page::Worktrees,
            s.s_cleanup,
            s.r_wt_unchanged,
            s.r_wt_unchanged_sub,
        ),
        (Page::Worktrees, s.s_cleanup, s.r_wt_keep, s.r_wt_keep_sub),
        (Page::Worktrees, s.s_wt_review, s.r_wt_scan, ""),
        (Page::Environments, s.s_machines, s.r_machines, ""),
        (Page::Access, s.s_access, s.r_server, s.r_server_sub),
        (Page::Access, s.s_access, s.r_expose, s.r_expose_sub),
        (Page::Access, s.s_access, s.r_events, s.r_events_sub),
        (Page::Access, s.s_access, s.r_writes, s.r_writes_sub),
        (Page::Access, s.s_access, s.r_images, s.r_images_sub),
        (Page::Access, s.s_access, s.r_floor, s.r_floor_sub),
        (Page::Maintenance, s.s_usage, s.r_savings, s.r_savings_sub),
        (Page::Maintenance, s.s_usage, s.r_bug, s.r_bug_sub),
        (Page::Labs, s.s_plugins, s.r_plugins, s.r_plugins_sub),
        (Page::About, s.s_build, s.r_version, ""),
        (Page::About, s.s_build, s.r_commit, ""),
        (Page::About, s.s_build, s.r_wire, s.r_wire_sub),
        (Page::About, s.s_build, s.r_core, ""),
        (Page::Archive, s.s_recover, s.r_recover, s.r_recover_sub),
    ];
    let mut items: Vec<Item> = rows
        .iter()
        .map(|(page, section, title, sub)| Item {
            page: *page,
            title: (*title).to_string(),
            terms: vec![
                (*sub).to_string(),
                (*section).to_string(),
                page_name_in(l, *page).to_string(),
            ],
            secondary: false,
        })
        .collect();
    items.extend(Page::all().map(|p| Item {
        page: p,
        title: page_name_in(l, p).to_string(),
        terms: vec![p.id().to_string()],
        secondary: true,
    }));
    items
}

impl Workspace {
    // ---- pieces --------------------------------------------------------

    /// A row: its title and plain line on the left, its control on the right.
    fn st_row(
        th: &theme::Theme,
        sk: &skin::Skin,
        title: &str,
        sub: Option<String>,
        control: Option<AnyElement>,
    ) -> gpui::Div {
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_4()
            .px_3()
            .py(px(9.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .flex_1()
                    .min_w(px(0.))
                    .child(
                        div()
                            .text_size(sk.text_px(10.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(th.text)
                            .child(SharedString::from(title.to_string())),
                    )
                    .when_some(sub.filter(|s| !s.is_empty()), |d, s| {
                        d.child(
                            div()
                                .text_size(sk.text_px(8.5))
                                .text_color(th.text.alpha(0.6))
                                .child(SharedString::from(s)),
                        )
                    }),
            )
            .children(control)
    }

    /// A section: its title in sentence case over a rounded group of rows
    /// with a hairline between each.
    fn st_section(
        th: &theme::Theme,
        sk: &skin::Skin,
        id: &'static str,
        title: &str,
        rows: Vec<gpui::Div>,
    ) -> AnyElement {
        let rule = px(sk.m.hairline.max(1.));
        div()
            .id(id)
            .debug_selector(move || id.into())
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(sk.text_px(11.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(th.text.alpha(0.92))
                    .child(SharedString::from(title.to_string())),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .rounded(sk.radius())
                    .border(sk.box_border(1.))
                    .border_color(th.text.alpha(0.14))
                    .bg(th.text.alpha(0.03))
                    .children(rows.into_iter().enumerate().map(move |(i, r)| {
                        r.when(i > 0, |d| d.border_t(rule).border_color(sk.ink.rule))
                    })),
            )
            .into_any_element()
    }

    fn st_switch(
        &self,
        th: &theme::Theme,
        sk1: &skin::Skin,
        id: &'static str,
        on: bool,
        flip: Flip,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .debug_selector(move || id.into())
            .cursor_pointer()
            .child(slider_switch(on, th, sk1))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    flip(ws, cx);
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    /// The button on a row whose setting still has a panel of its own. With a
    /// `host` the panel opens in place, on this page of the book; without one
    /// (the composer, the Threshing Floor, which are whole surfaces) the book
    /// closes and the surface opens.
    fn st_open(
        &self,
        th: &theme::Theme,
        sk: &skin::Skin,
        id: &'static str,
        host: Option<Host>,
        then: Then,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let acc = th.accent;
        div()
            .id(id)
            .debug_selector(move || id.into())
            .flex_shrink_0()
            .px_3()
            .py_1()
            .rounded(sk.radius())
            .border(sk.box_border(1.))
            .border_color(th.text.alpha(0.22))
            .text_size(sk.text_px(9.5))
            .text_color(th.text.alpha(0.9))
            .cursor_pointer()
            .hover(move |s| s.bg(acc.alpha(0.14)).border_color(acc.alpha(0.7)))
            .child(SharedString::from(format!(
                "{} \u{2197}",
                self.lang.strings().settings.open
            )))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _: &MouseDownEvent, w, cx| {
                    cx.stop_propagation();
                    match host {
                        Some(h) => {
                            ws.unhost();
                            if let Some(b) = ws.settings.as_mut() {
                                b.host = Some(h);
                            }
                        }
                        None => ws.close_settings(cx),
                    }
                    then(ws, w, cx);
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    /// A button that acts inside the book.
    fn st_button(
        th: &theme::Theme,
        sk: &skin::Skin,
        id: &'static str,
        label: String,
        lit: bool,
        flip: Flip,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let acc = th.accent;
        div()
            .id(id)
            .debug_selector(move || id.into())
            .flex_shrink_0()
            .px_3()
            .py_1()
            .rounded(sk.radius())
            .border(sk.box_border(1.))
            .border_color(if lit { acc } else { th.text.alpha(0.22) })
            .when(lit, |d| d.bg(acc.alpha(0.18)))
            .text_size(sk.text_px(9.5))
            .text_color(th.text.alpha(0.9))
            .cursor_pointer()
            .hover(move |st| st.bg(acc.alpha(0.14)).border_color(acc.alpha(0.7)))
            .child(SharedString::from(label))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    flip(ws, cx);
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    /// The Worktrees page: T3's Cleanup rules, then the review of what they
    /// would remove, with the remove button that needs a second press.
    fn wt_page(
        &self,
        th: &theme::Theme,
        sk: &skin::Skin,
        sk1: &skin::Skin,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let s = self.lang.strings().settings;
        let p = &self.wt_prefs;
        let acc = th.accent;
        let step = |id: &'static str, label: &str, delta: i32| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px_2()
                .rounded(sk.radius())
                .border(sk.box_border(1.))
                .border_color(th.text.alpha(0.22))
                .text_size(sk.text_px(10.))
                .cursor_pointer()
                .hover(move |st| st.bg(acc.alpha(0.14)))
                .child(SharedString::from(label.to_string()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        let d = ws.wt_prefs.days() as i32 + delta;
                        ws.wt_prefs.inactive_days = Some(d.clamp(1, 365) as u32);
                        ws.wt_save();
                        cx.notify();
                    }),
                )
        };
        let stepper = div()
            .flex()
            .items_center()
            .gap_2()
            .child(step("st-wt-days-less", "\u{2212}", -1))
            .child(
                div()
                    .min_w(px(52.))
                    .text_size(sk.text_px(9.5))
                    .text_color(th.text)
                    .child(SharedString::from(lang::fill(
                        s.wt_days,
                        &[("n", &p.days().to_string())],
                    ))),
            )
            .child(step("st-wt-days-more", "+", 1));
        let cleanup = vec![
            Self::st_row(
                th,
                sk,
                s.r_wt_merged,
                Some(s.r_wt_merged_sub.into()),
                Some(self.st_switch(
                    th,
                    sk1,
                    "st-wt-merged",
                    p.merged_on(),
                    |ws, _| {
                        ws.wt_prefs.merged = Some(!ws.wt_prefs.merged_on());
                        ws.wt_save();
                    },
                    cx,
                )),
            ),
            Self::st_row(
                th,
                sk,
                s.r_wt_inactive,
                Some(s.r_wt_inactive_sub.into()),
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .child(stepper)
                        .child(self.st_switch(
                            th,
                            sk1,
                            "st-wt-inactive",
                            p.inactive_on(),
                            |ws, _| {
                                ws.wt_prefs.inactive = Some(!ws.wt_prefs.inactive_on());
                                ws.wt_save();
                            },
                            cx,
                        ))
                        .into_any_element(),
                ),
            ),
            Self::st_row(
                th,
                sk,
                s.r_wt_unchanged,
                Some(s.r_wt_unchanged_sub.into()),
                Some(self.st_switch(
                    th,
                    sk1,
                    "st-wt-unchanged",
                    p.unchanged_on(),
                    |ws, _| {
                        ws.wt_prefs.unchanged = Some(!ws.wt_prefs.unchanged_on());
                        ws.wt_save();
                    },
                    cx,
                )),
            ),
            Self::st_row(
                th,
                sk,
                s.r_wt_keep,
                Some(s.r_wt_keep_sub.into()),
                Some(Self::st_value(th, sk, s.wt_always.into())),
            ),
        ];
        // The review.
        let r = &self.wt_review;
        let home = session::home_dir();
        let short = |path: &Path| match path.strip_prefix(&home) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        };
        let now = SystemTime::now();
        let mut review = Vec::new();
        let (status, judged): (String, Vec<(&wtclean::Facts, wtclean::Verdict)>) =
            match (&r.got, r.busy) {
                (_, true) => (s.wt_busy.into(), Vec::new()),
                (None, false) => (s.wt_never.into(), Vec::new()),
                (Some(Err(e)), false) => (e.clone(), Vec::new()),
                (Some(Ok((facts, repos))), false) => {
                    let judged: Vec<_> = facts
                        .iter()
                        .filter(|f| !f.primary)
                        .map(|f| (f, wtclean::judge(f, p, now)))
                        .collect();
                    let remove = judged.iter().filter(|(_, v)| v.removes()).count();
                    (
                        lang::fill(
                            s.wt_found,
                            &[
                                ("n", &judged.len().to_string()),
                                ("repos", &repos.to_string()),
                                ("remove", &remove.to_string()),
                            ],
                        ),
                        judged,
                    )
                }
            };
        review.push(Self::st_row(
            th,
            sk,
            s.r_wt_scan,
            Some(status),
            Some(Self::st_button(
                th,
                sk,
                "st-wt-look",
                s.wt_look.into(),
                false,
                |ws, cx| ws.wt_look(cx),
                cx,
            )),
        ));
        let mut kept: std::collections::BTreeMap<wtclean::Keep, usize> = Default::default();
        let mut n_remove = 0usize;
        for (f, v) in &judged {
            match v {
                wtclean::Verdict::Keep(k) => *kept.entry(*k).or_default() += 1,
                wtclean::Verdict::Remove(whys) => {
                    n_remove += 1;
                    let mut sub: Vec<String> = Vec::new();
                    if let Some(b) = &f.branch {
                        sub.push(b.clone());
                    }
                    for w in whys {
                        sub.push(match w {
                            wtclean::Why::Merged => s.wt_why_merged.into(),
                            wtclean::Why::Inactive { days } => {
                                lang::fill(s.wt_why_inactive, &[("n", &days.to_string())])
                            }
                            wtclean::Why::Unchanged => lang::fill(
                                s.wt_why_unchanged,
                                &[("base", f.base.as_deref().unwrap_or("?"))],
                            ),
                        });
                    }
                    let mut text = sub.join(" \u{b7} ");
                    if !f.ignored.is_empty() {
                        text = format!(
                            "{text}\n{}",
                            lang::fill(s.wt_ignored, &[("names", &f.ignored.join(", "))])
                        );
                    }
                    review.push(Self::st_row(th, sk, &short(&f.path), Some(text), None));
                }
            }
        }
        if !kept.is_empty() {
            let list: Vec<String> = kept
                .iter()
                .map(|(k, n)| {
                    let word = match k {
                        wtclean::Keep::Changes => s.wt_keep_changes,
                        wtclean::Keep::Occupied => s.wt_keep_occupied,
                        wtclean::Keep::Locked => s.wt_keep_locked,
                        wtclean::Keep::Unread => s.wt_keep_unread,
                        wtclean::Keep::NoRule | wtclean::Keep::Primary => s.wt_keep_norule,
                    };
                    lang::fill(word, &[("n", &n.to_string())])
                })
                .collect();
            review.push(Self::st_row(
                th,
                sk,
                &lang::fill(s.wt_kept, &[("list", &list.join(", "))]),
                None,
                None,
            ));
        }
        if !p.any_on() {
            review.push(Self::st_row(th, sk, s.wt_rules_off, None, None));
        }
        if n_remove > 0 {
            let label = if r.armed {
                lang::fill(s.wt_remove_again, &[("n", &n_remove.to_string())])
            } else {
                lang::fill(s.wt_remove, &[("n", &n_remove.to_string())])
            };
            review.push(
                div()
                    .flex()
                    .justify_end()
                    .px_3()
                    .py_2()
                    .child(Self::st_button(
                        th,
                        sk,
                        "st-wt-remove",
                        label,
                        r.armed,
                        |ws, cx| ws.wt_remove_press(cx),
                        cx,
                    )),
            );
        }
        if let Some(said) = &r.said {
            review.push(Self::st_row(th, sk, said, None, None));
        }
        vec![
            Self::st_section(th, sk, "st-sec-cleanup", s.s_cleanup, cleanup),
            Self::st_section(th, sk, "st-sec-wt-review", s.s_wt_review, review),
        ]
    }

    /// A read-only value on the right of a row.
    fn st_value(th: &theme::Theme, sk: &skin::Skin, text: String) -> AnyElement {
        div()
            .flex_shrink_0()
            .max_w(px(320.))
            .text_size(sk.text_px(9.))
            .text_color(th.text.alpha(0.75))
            .child(SharedString::from(text))
            .into_any_element()
    }

    // ---- the pages -----------------------------------------------------

    fn settings_page(
        &self,
        page: Page,
        th: &theme::Theme,
        sk: &skin::Skin,
        sk1: &skin::Skin,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let s = self.lang.strings().settings;
        let t = self.lang.strings();
        let ui = t.ui;
        let row = |title: &str, sub: &str, c: AnyElement| {
            Self::st_row(th, sk, title, Some(sub.to_string()), Some(c))
        };
        match page {
            Page::General => vec![
                Self::st_section(
                    th,
                    sk,
                    "st-sec-language",
                    s.s_language,
                    vec![row(
                        s.r_language,
                        s.r_language_sub,
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(Self::st_value(th, sk, self.lang.native().to_string()))
                            .child(self.st_open(
                                th,
                                sk,
                                "st-open-language",
                                Some(Host::Language),
                                |ws, _, _| {
                                    ws.lang_picker = Some(LangPicker {
                                        query: EditBuffer::seeded(""),
                                        selected: 0,
                                    })
                                },
                                cx,
                            ))
                            .into_any_element(),
                    )],
                ),
                Self::st_section(
                    th,
                    sk,
                    "st-sec-window",
                    s.s_window,
                    vec![
                        row(
                            s.r_scale,
                            s.r_scale_sub,
                            self.st_open(
                                th,
                                sk,
                                "st-open-scale",
                                Some(Host::Scale),
                                |ws, _, _| ws.scale_menu = true,
                                cx,
                            ),
                        ),
                        row(
                            s.r_updates,
                            s.r_updates_sub,
                            self.st_open(
                                th,
                                sk,
                                "st-open-updates",
                                Some(Host::Updates),
                                |ws, _, cx| ws.open_updates(cx),
                                cx,
                            ),
                        ),
                        row(
                            s.r_admin,
                            s.r_admin_sub,
                            self.st_open(
                                th,
                                sk,
                                "st-open-admin",
                                Some(Host::Admin),
                                |ws, _, cx| ws.open_admin(cx),
                                cx,
                            ),
                        ),
                    ],
                ),
            ],
            Page::Appearance => vec![
                Self::st_section(
                    th,
                    sk,
                    "st-sec-look",
                    s.s_look,
                    vec![
                        row(
                            s.r_palette,
                            s.r_palette_sub,
                            self.st_open(
                                th,
                                sk,
                                "st-open-palette",
                                Some(Host::Palette),
                                |ws, _, _| ws.theme_menu = Some(MenuScope::Outer),
                                cx,
                            ),
                        ),
                        row(
                            s.r_gauges,
                            s.r_gauges_sub,
                            self.st_open(
                                th,
                                sk,
                                "st-open-gauges",
                                Some(Host::Gauges),
                                |ws, _, _| ws.open_osd(MenuScope::Outer, None),
                                cx,
                            ),
                        ),
                    ],
                ),
                Self::st_section(
                    th,
                    sk,
                    "st-sec-agent-panes",
                    s.s_agent_panes,
                    vec![row(
                        s.r_tint,
                        s.r_tint_sub,
                        self.st_switch(
                            th,
                            sk1,
                            "st-tint",
                            !self.agent_tint,
                            |ws, cx| ws.toggle_agent_tint(cx),
                            cx,
                        ),
                    )],
                ),
            ],
            Page::Notifications => vec![Self::st_section(
                th,
                sk,
                "st-sec-notices",
                s.s_notices,
                vec![row(
                    s.r_notices,
                    s.r_notices_sub,
                    self.st_open(
                        th,
                        sk,
                        "st-open-notices",
                        Some(Host::Notices),
                        |ws, _, _| {
                            ws.notif = notifpref::load();
                            ws.notif_menu = true;
                        },
                        cx,
                    ),
                )],
            )],
            Page::Keybindings => vec![Self::st_section(
                th,
                sk,
                "st-sec-keys",
                s.s_keys,
                vec![
                    row(
                        s.r_keys,
                        s.r_keys_sub,
                        self.st_open(
                            th,
                            sk,
                            "st-open-keys",
                            Some(Host::Keys),
                            |ws, _, _| ws.help_open = true,
                            cx,
                        ),
                    ),
                    row(
                        s.r_settings_key,
                        s.r_settings_key_sub,
                        Self::st_value(th, sk, "Ctrl+,".into()),
                    ),
                ],
            )],
            Page::Providers => {
                let local = launcher::LocalAdapter::config_path();
                vec![
                    Self::st_section(
                        th,
                        sk,
                        "st-sec-skills",
                        s.s_skills,
                        vec![
                            row(
                                s.r_skills,
                                s.r_skills_sub,
                                self.st_switch(
                                    th,
                                    sk1,
                                    "st-skills",
                                    self.skills_prefs.active(),
                                    |ws, cx| {
                                        ws.change_skill(0, false, cx);
                                        if !ws.skills_prefs.active() {
                                            ws.cancel_skill_import();
                                        }
                                    },
                                    cx,
                                ),
                            ),
                            row(
                                s.r_skill_list,
                                s.r_skill_list_sub,
                                self.st_open(
                                    th,
                                    sk,
                                    "st-open-skills",
                                    Some(Host::Skills),
                                    |ws, _, cx| ws.open_agents(cx),
                                    cx,
                                ),
                            ),
                        ],
                    ),
                    Self::st_section(
                        th,
                        sk,
                        "st-sec-launch",
                        s.s_launch,
                        vec![
                            div().p_2().child(self.render_launch_defaults(th, cx)),
                            Self::st_row(
                                th,
                                sk,
                                s.r_local,
                                Some(local.display().to_string()),
                                None,
                            ),
                        ],
                    ),
                ]
            }
            Page::Threads => {
                let p = &self.agents_prefs;
                let c = &self.rest_census;
                let now = lang::fill(
                    ui.ag_now,
                    &[
                        ("resting", &c.memory.resting.panes().to_string()),
                        ("asleep", &c.memory.asleep.panes().to_string()),
                        (
                            "back",
                            &c.memory.decrease.label(t.parts.pane_mem_unmeasured),
                        ),
                        ("held", &c.memory.held.label(t.parts.pane_mem_unmeasured)),
                    ],
                );
                vec![Self::st_section(
                    th,
                    sk,
                    "st-sec-rest",
                    s.s_rest,
                    vec![
                        row(
                            ui.ag_rest_name,
                            ui.ag_rest_blurb,
                            self.st_switch(
                                th,
                                sk1,
                                "st-rest",
                                p.rest_on(),
                                |ws, _| {
                                    ws.agents_prefs.rest = Some(!ws.agents_prefs.rest_on());
                                    agentcfg::save(&ws.agents_prefs);
                                },
                                cx,
                            ),
                        ),
                        Self::st_row(
                            th,
                            sk,
                            ui.ag_rest_after,
                            Some(self.agents_minutes_label(p.rest_after_min())),
                            Some(self.agents_slider(0, None, th, sk, cx).into_any_element()),
                        ),
                        row(
                            ui.ag_sleep_name,
                            ui.ag_sleep_blurb,
                            self.st_switch(
                                th,
                                sk1,
                                "st-sleep",
                                p.sleep_on(),
                                |ws, _| {
                                    let on = !ws.agents_prefs.sleep_on();
                                    ws.agents_prefs.sleep = Some(on);
                                    // Sleep needs rest: switching sleep on switches rest on.
                                    if on {
                                        ws.agents_prefs.rest = Some(true);
                                    }
                                    agentcfg::save(&ws.agents_prefs);
                                },
                                cx,
                            ),
                        ),
                        Self::st_row(
                            th,
                            sk,
                            ui.ag_sleep_after,
                            Some(self.agents_minutes_label(p.sleep_after_rest_min())),
                            Some(self.agents_slider(1, None, th, sk, cx).into_any_element()),
                        ),
                        Self::st_row(
                            th,
                            sk,
                            s.r_now,
                            Some(format!("{now}\n{}", ui.am_coverage)),
                            None,
                        ),
                    ],
                )]
            }
            Page::Projects => {
                let file = addproject::projects_path(&instance::config_dir());
                vec![Self::st_section(
                    th,
                    sk,
                    "st-sec-projects",
                    s.s_projects,
                    vec![
                        row(
                            s.r_add_project,
                            s.r_add_project_sub,
                            self.st_open(
                                th,
                                sk,
                                "st-open-add-project",
                                None,
                                |ws, w, cx| {
                                    ws.open_composer(w, cx);
                                    ws.add_project_open(cx);
                                },
                                cx,
                            ),
                        ),
                        Self::st_row(
                            th,
                            sk,
                            s.r_projects_file,
                            Some(file.display().to_string()),
                            None,
                        ),
                    ],
                )]
            }
            Page::Worktrees => self.wt_page(th, sk, sk1, cx),
            Page::Environments => {
                let config = instance::config_dir();
                let path = entry::machines_path(&config);
                let n = match entry::load_machines(&config) {
                    Ok(m) => m.len().to_string(),
                    Err(e) => e,
                };
                vec![Self::st_section(
                    th,
                    sk,
                    "st-sec-machines",
                    s.s_machines,
                    vec![row(
                        s.r_machines,
                        &lang::fill(
                            s.r_machines_sub,
                            &[("n", &n), ("path", &path.display().to_string())],
                        ),
                        self.st_open(
                            th,
                            sk,
                            "st-open-machines",
                            None,
                            |ws, w, cx| {
                                ws.open_composer(w, cx);
                                ws.add_project_open(cx);
                            },
                            cx,
                        ),
                    )],
                )]
            }
            Page::Access => {
                let expose = self.mcp.expose;
                let chip = |id: &'static str, label: &str, lit: bool, to: mcp::Expose| {
                    let acc = th.accent;
                    div()
                        .id(id)
                        .debug_selector(move || id.into())
                        .px_2()
                        .py_0p5()
                        .rounded(sk.radius())
                        .border(sk.box_border(1.))
                        .border_color(if lit { acc } else { th.text.alpha(0.18) })
                        .when(lit, |d| d.bg(acc.alpha(0.16)))
                        .text_size(sk.text_px(9.))
                        .text_color(if lit { th.text } else { th.text.alpha(0.65) })
                        .cursor_pointer()
                        .child(SharedString::from(label.to_string()))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.mcp.expose = to;
                                ws.save(cx);
                                cx.notify();
                            }),
                        )
                };
                let picker = div()
                    .flex()
                    .gap_1()
                    .child(chip(
                        "st-expose-agents",
                        t.exp_agents,
                        expose == mcp::Expose::AgentsOnly,
                        mcp::Expose::AgentsOnly,
                    ))
                    .child(chip(
                        "st-expose-all",
                        t.exp_all,
                        expose == mcp::Expose::All,
                        mcp::Expose::All,
                    ))
                    .into_any_element();
                vec![Self::st_section(
                    th,
                    sk,
                    "st-sec-access",
                    s.s_access,
                    vec![
                        row(
                            s.r_server,
                            s.r_server_sub,
                            self.st_switch(
                                th,
                                sk1,
                                "st-mcp-server",
                                self.mcp.enabled,
                                |ws, cx| {
                                    ws.mcp.enabled = !ws.mcp.enabled;
                                    ws.save(cx);
                                },
                                cx,
                            ),
                        ),
                        row(s.r_expose, s.r_expose_sub, picker),
                        row(
                            s.r_events,
                            s.r_events_sub,
                            self.st_switch(
                                th,
                                sk1,
                                "st-mcp-events",
                                self.mcp.events,
                                |ws, cx| {
                                    ws.mcp.events = !ws.mcp.events;
                                    ws.save(cx);
                                },
                                cx,
                            ),
                        ),
                        row(
                            s.r_writes,
                            s.r_writes_sub,
                            self.st_switch(
                                th,
                                sk1,
                                "st-mcp-writes",
                                self.mcp.writable,
                                |ws, cx| {
                                    ws.mcp.writable = !ws.mcp.writable;
                                    ws.save(cx);
                                },
                                cx,
                            ),
                        ),
                        row(
                            s.r_images,
                            s.r_images_sub,
                            self.st_switch(
                                th,
                                sk1,
                                "st-mcp-images",
                                wall_card_art(self.card_art),
                                |ws, cx| ws.toggle_card_art(cx),
                                cx,
                            ),
                        ),
                        row(
                            s.r_floor,
                            s.r_floor_sub,
                            self.st_open(
                                th,
                                sk,
                                "st-open-floor",
                                None,
                                |ws, _, _| {
                                    ws.mcp_menu = false;
                                    ws.threshing_floor = true;
                                    ws.ask_numbers =
                                        askrecord::numbers_for(crate::instance::key()).ok();
                                },
                                cx,
                            ),
                        ),
                    ],
                )]
            }
            Page::Maintenance => vec![Self::st_section(
                th,
                sk,
                "st-sec-usage",
                s.s_usage,
                vec![
                    row(
                        s.r_savings,
                        s.r_savings_sub,
                        self.st_open(
                            th,
                            sk,
                            "st-open-savings",
                            Some(Host::Savings),
                            |ws, _, cx| ws.fetch_savings(None, cx),
                            cx,
                        ),
                    ),
                    row(
                        s.r_bug,
                        s.r_bug_sub,
                        self.st_open(
                            th,
                            sk,
                            "st-open-bug",
                            Some(Host::Bug),
                            |ws, _, _| ws.open_bug_menu(),
                            cx,
                        ),
                    ),
                ],
            )],
            Page::Labs => vec![Self::st_section(
                th,
                sk,
                "st-sec-plugins",
                s.s_plugins,
                vec![row(
                    s.r_plugins,
                    s.r_plugins_sub,
                    self.st_open(
                        th,
                        sk,
                        "st-open-plugins",
                        Some(Host::Plugins),
                        |ws, _, _| ws.plugins_menu = true,
                        cx,
                    ),
                )],
            )],
            Page::About => {
                let b = update::BuildInfo::this();
                let unknown = t.parts.update_commit_unknown.to_string();
                let mut commit = b.short_sha().map(str::to_string).unwrap_or(unknown.clone());
                if b.dirty == Some(true) {
                    commit.push_str("+dirty");
                }
                vec![Self::st_section(
                    th,
                    sk,
                    "st-sec-build",
                    s.s_build,
                    vec![
                        Self::st_row(
                            th,
                            sk,
                            s.r_version,
                            None,
                            Some(Self::st_value(th, sk, b.version.clone())),
                        ),
                        Self::st_row(
                            th,
                            sk,
                            s.r_commit,
                            None,
                            Some(Self::st_value(th, sk, commit)),
                        ),
                        Self::st_row(
                            th,
                            sk,
                            s.r_wire,
                            Some(s.r_wire_sub.to_string()),
                            Some(Self::st_value(th, sk, b.wire.clone().unwrap_or(unknown))),
                        ),
                        Self::st_row(
                            th,
                            sk,
                            s.r_core,
                            None,
                            Some(Self::st_value(th, sk, b.core.clone())),
                        ),
                    ],
                )]
            }
            Page::Archive => vec![Self::st_section(
                th,
                sk,
                "st-sec-recover",
                s.s_recover,
                vec![row(
                    s.r_recover,
                    s.r_recover_sub,
                    self.st_open(
                        th,
                        sk,
                        "st-open-recover",
                        Some(Host::Recover),
                        |ws, _, _| {
                            ws.dead_filter = None;
                            ws.dead_menu = true;
                        },
                        cx,
                    ),
                )],
            )],
        }
    }

    // ---- the book ------------------------------------------------------

    fn settings_nav(
        &self,
        b: &Book,
        th: &theme::Theme,
        sk: &skin::Skin,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let s = self.lang.strings().settings;
        let acc = th.accent;
        let item = |id: SharedString,
                    glyph: &'static str,
                    word: &'static str,
                    label: &str,
                    lit: bool,
                    page: Page| {
            div()
                .id(id.clone())
                .debug_selector(move || id.to_string())
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .py(px(5.))
                .rounded(sk.radius())
                .when(lit, |d| d.bg(acc.alpha(0.18)))
                .text_size(sk.text_px(10.))
                .text_color(if lit { th.text } else { th.text.alpha(0.78) })
                .cursor_pointer()
                .hover(move |st| st.bg(acc.alpha(0.10)))
                .child(
                    div()
                        .w(px(18.))
                        .flex_shrink_0()
                        .text_size(sk.text_px(11.))
                        .child(sk.control(glyph, word)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .truncate()
                        .child(SharedString::from(label.to_string())),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.settings_go(page, cx);
                    }),
                )
        };
        let search = div()
            .id("st-search")
            .debug_selector(|| "st-search".into())
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .mb_2()
            .rounded(sk.radius())
            .border(sk.box_border(1.))
            .border_color(if b.searching() {
                acc.alpha(0.7)
            } else {
                th.text.alpha(0.16)
            })
            .text_size(sk.text_px(10.))
            .child(div().text_color(th.text.alpha(0.5)).child("\u{2315}"))
            .child(if b.searching() {
                div()
                    .flex_1()
                    .text_color(th.text)
                    .child(SharedString::from(format!("{}\u{258f}", b.query)))
            } else {
                div()
                    .flex_1()
                    .text_color(th.text.alpha(0.45))
                    .child(SharedString::from(s.search.to_string()))
            });
        let mut list = div().flex().flex_col().gap_0p5();
        if b.searching() {
            let hits = self.settings_hits();
            if hits.is_empty() {
                list = list.child(
                    div()
                        .px_2()
                        .py_2()
                        .text_size(sk.text_px(9.))
                        .text_color(th.text.alpha(0.5))
                        .child(SharedString::from(s.no_match.to_string())),
                );
            }
            for (i, hit) in hits.iter().enumerate().take(40) {
                let lit = i == b.pick;
                let page = hit.page;
                let label = if hit.secondary {
                    hit.title.clone()
                } else {
                    format!("{} \u{203a} {}", self.page_name(page), hit.title)
                };
                list = list.child(item(
                    SharedString::from(format!("st-hit-{i}")),
                    page.glyph(),
                    self.page_name(page),
                    &label,
                    lit,
                    page,
                ));
            }
        } else {
            for (gi, (heading, pages)) in settings::GROUPS.iter().enumerate() {
                if gi > 0 {
                    list = list.child(div().h(px(10.)));
                }
                if let Some(h) = heading {
                    list = list.child(
                        div()
                            .px_2()
                            .pb_1()
                            .text_size(sk.text_px(8.5))
                            .text_color(th.text.alpha(0.5))
                            .child(SharedString::from(self.heading_name(*h).to_string())),
                    );
                }
                for p in pages.iter() {
                    list = list.child(item(
                        SharedString::from(format!("st-nav-{}", p.id())),
                        p.glyph(),
                        self.page_name(*p),
                        self.page_name(*p),
                        *p == b.page,
                        *p,
                    ));
                }
            }
        }
        div()
            .id("st-nav")
            .debug_selector(|| "st-nav".into())
            .w(px(NAV_W))
            .flex_shrink_0()
            .h_full()
            .p_3()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .border_r(px(sk.m.hairline.max(1.)))
            .border_color(sk.ink.rule)
            .child(search)
            .child(list)
    }

    pub(crate) fn render_settings(
        &self,
        th: &theme::Theme,
        hosted: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        let b = self.settings.clone()?;
        let s = self.lang.strings().settings;
        let sk = skin::for_theme(cx, th, th.grade.scale);
        let sk1 = skin::for_theme(cx, th, 1.);
        let (vw, vh) = self
            .last_win
            .map(|(_, _, w, h)| (w, h))
            .unwrap_or((1200., 800.));
        // Nearly the whole window: the surface config alone is four columns
        // wide, and it opens on this page now.
        let w = (vw - 48.).max(320.);
        let h = (vh - 64.).max(300.);
        let host = self.hosting();
        let acc = th.accent;
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .pb_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(sk.text_px(10.))
                    .child(
                        div()
                            .text_color(th.text.alpha(0.55))
                            .child(SharedString::from(s.title.to_string())),
                    )
                    .child(div().text_color(th.text.alpha(0.35)).child("/"))
                    .child(
                        div()
                            .id("st-crumb")
                            .debug_selector(|| "st-crumb".into())
                            .when(host.is_some(), |d| {
                                d.cursor_pointer()
                                    .hover(move |st| st.text_color(acc))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                            cx.stop_propagation();
                                            ws.unhost();
                                            cx.notify();
                                        }),
                                    )
                            })
                            .text_color(if host.is_some() {
                                th.text.alpha(0.55)
                            } else {
                                th.text
                            })
                            .font_weight(if host.is_some() {
                                gpui::FontWeight::NORMAL
                            } else {
                                gpui::FontWeight::SEMIBOLD
                            })
                            .child(SharedString::from(self.page_name(b.page).to_string())),
                    )
                    .when_some(host, |d, h| {
                        d.child(div().text_color(th.text.alpha(0.35)).child("/"))
                            .child(
                                div()
                                    .id("st-crumb-host")
                                    .debug_selector(|| "st-crumb-host".into())
                                    .text_color(th.text)
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(SharedString::from(self.host_title(h).to_string())),
                            )
                    }),
            )
            .child(
                with_tip(
                    div()
                        .id("st-close")
                        .debug_selector(|| "st-close".into())
                        .px_2()
                        .rounded(sk.radius())
                        .text_size(sk.text_px(12.))
                        .text_color(th.text.alpha(0.7))
                        .cursor_pointer()
                        .hover(move |st| st.bg(acc.alpha(0.14)))
                        .child("\u{2715}"),
                    vec![format!("{} · Esc", s.close)],
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.close_settings(cx);
                    }),
                ),
            );
        let body = div()
            .id("st-page")
            .debug_selector(|| "st-page".into())
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .px_5()
            .pt_3()
            .pb_5()
            .flex()
            .flex_col()
            .gap_5()
            .overflow_y_scroll()
            .child(header)
            .map(|d| match hosted {
                // The panel, in place: it fills the page, and a press on the
                // page around it closes it back to the rows.
                Some(el) => d.child(
                    div()
                        .id("st-hosted")
                        .debug_selector(|| "st-hosted".into())
                        .relative()
                        .flex_1()
                        .min_h(px(0.))
                        .child(el),
                ),
                None => d.children(self.settings_page(b.page, th, &sk, &sk1, cx)),
            });
        let book = div()
            .id("settings-book")
            .debug_selector(|| "settings-book".into())
            .w(px(w))
            .h(px(h))
            .overflow_hidden()
            .rounded(sk.radius_lg())
            .border(sk.box_border(2.))
            .border_color(acc.alpha(0.85))
            .bg(darken(th.surface, 0.6))
            .shadow(float_shadows(acc, th.grade.phosphor))
            .flex()
            .flex_row()
            .text_color(th.text)
            .map(|d| sk.chrome(sk.popup(sk.finish(d))))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .child(self.settings_nav(&b, th, &sk, cx))
            .child(body)
            // Everything behind the book is frosted while it is open, and the
            // book itself stays sharp: the same post-pass the FOCUS reader
            // and the agent wall use.
            .child(frost_behind());
        Some(
            div()
                .absolute()
                .inset_0()
                .on_children_prepainted(keep_panels_from_doc_grade)
                .bg(th.bg.alpha(0.70))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| ws.close_settings(cx)),
                )
                .child(book),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every page has rows the search can find, in every language, and
    /// every row's page is one the nav lists: the search index is written
    /// beside the pages, and this is what keeps the two from drifting.
    #[test]
    fn every_page_has_rows_the_search_can_find() {
        for l in lang::Lang::ALL.iter() {
            let items = settings_items_in(*l);
            for p in Page::all() {
                assert!(
                    items.iter().any(|i| i.page == p && !i.secondary),
                    "{l:?}: {p:?} has no row to find"
                );
                let name = page_name_in(*l, p);
                assert!(!name.is_empty());
                let hits = settings::search(name, &items);
                assert!(
                    hits.iter().any(|&i| items[i].page == p),
                    "{l:?}: searching {name:?} does not reach {p:?}"
                );
            }
        }
        let en = settings_items_in(lang::Lang::En);
        let hit = settings::search("mcp", &en);
        assert_eq!(
            en[hit[0]].page,
            Page::Access,
            "MCP lands on Sharing & access"
        );
        let hit = settings::search("sleep", &en);
        assert_eq!(en[hit[0]].page, Page::Threads);
    }

    fn book_window(
        cx: &mut gpui::TestAppContext,
    ) -> (gpui::Entity<Workspace>, &mut gpui::VisualTestContext) {
        cx.executor().allow_parking();
        cx.update(|cx| {
            theme::init_embedded(cx, theme::house_outer());
            skin::init_embedded(cx);
        });
        let seed = session::PaneRestore {
            program: Some(("/bin/sh".into(), vec!["-c".into(), "exec /bin/cat".into()])),
            ..Default::default()
        };
        let window = {
            let _fork = crate::testsync::forks_and_locks();
            cx.open_window(size(px(1400.), px(900.)), |w, cx| {
                Workspace::build(true, true, Some(seed), None, w, cx)
            })
        };
        let ws = window.root(cx).unwrap();
        let cx = gpui::VisualTestContext::from_window(window.into(), cx).into_mut();
        let stops: Vec<_> = ws
            .read_with(cx, |ws, _| ws.every_view())
            .into_iter()
            .map(|p| p.read_with(cx, |v, _| v.fixture_shutdown()))
            .collect();
        cx.on_quit(move || stops.into_iter().for_each(|s| s()));
        (ws, cx)
    }

    fn redraw(cx: &mut gpui::VisualTestContext) {
        for _ in 0..2 {
            cx.update(|w, _| w.refresh());
            cx.run_until_parked();
        }
    }

    /// Ctrl+, from a pane opens the book, the ⚙ is in the corner, typing
    /// searches and Enter goes to the lit result, the arrows walk the pages,
    /// Esc clears a search before it closes the book, and the book opens
    /// again on the page it was left on.
    ///
    /// The pane keeps the focus it had when Ctrl+, was pressed, so the
    /// book takes its keys in the capture phase; this test is what first
    /// caught letters typed into the search reaching the shell as well.
    ///
    /// Mutation-tested: dropping the capture-phase call leaves the search
    /// empty; forgetting the page in `settings_peel` fails the last
    /// assertion.
    #[gpui::test]
    fn ctrl_comma_opens_the_book_and_its_keyboard_searches_walks_and_peels(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx) = book_window(cx);
        redraw(cx);
        assert!(
            cx.debug_bounds("toolbar-settings").is_some(),
            "the ⚙ leads the corner"
        );
        // Prove the terminal takes typing before the book opens: it refuses
        // keys for its first 150ms, so a quiet shell would prove nothing.
        let pane = ws.read_with(cx, |ws, _| ws.every_view()[0].clone());
        cx.update(|w, cx| ws.update(cx, |ws, cx| ws.focus_active(w, cx)));
        let screen_has = |cx: &mut gpui::VisualTestContext, text: &str, tries: usize| {
            for _ in 0..tries {
                redraw(cx);
                if pane.read_with(cx, |v, _| v.fixture_screen_text().contains(text)) {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            false
        };
        std::thread::sleep(std::time::Duration::from_millis(200));
        cx.simulate_keystrokes("a b");
        assert!(screen_has(cx, "ab", 1000), "the terminal takes typing");
        cx.simulate_keystrokes("ctrl-,");
        redraw(cx);
        let page = |cx: &mut gpui::VisualTestContext| {
            ws.read_with(cx, |ws, _| {
                ws.settings.as_ref().map(|b| (b.page, b.query.clone()))
            })
        };
        assert_eq!(page(cx), Some((Page::General, String::new())));
        assert!(
            cx.debug_bounds("settings-book").is_some(),
            "the book is drawn"
        );
        assert!(cx.debug_bounds("st-nav-threads").is_some(), "with its nav");
        cx.simulate_keystrokes("s l e e p");
        redraw(cx);
        assert_eq!(
            page(cx),
            Some((Page::General, "sleep".into())),
            "typing searches"
        );
        assert!(
            cx.debug_bounds("st-hit-0").is_some(),
            "and lists what it found"
        );
        assert!(
            !screen_has(cx, "sleep", 40),
            "a letter typed into the search reached the shell behind the book"
        );
        cx.simulate_keystrokes("enter");
        redraw(cx);
        assert_eq!(
            page(cx),
            Some((Page::Threads, String::new())),
            "Enter goes there"
        );
        assert!(
            cx.debug_bounds("st-rest").is_some(),
            "the Threads page is drawn"
        );
        cx.simulate_keystrokes("down");
        assert_eq!(page(cx).map(|p| p.0), Some(Page::Projects), "down walks on");
        cx.simulate_keystrokes("x");
        cx.simulate_keystrokes("escape");
        assert_eq!(
            page(cx),
            Some((Page::Projects, String::new())),
            "Esc clears first"
        );
        cx.simulate_keystrokes("escape");
        assert_eq!(page(cx), None, "then closes");
        redraw(cx);
        cx.simulate_keystrokes("ctrl-,");
        redraw(cx);
        assert_eq!(
            page(cx).map(|p| p.0),
            Some(Page::Projects),
            "and reopens where it was"
        );
    }

    /// The Worktrees page on a real repository: the look finds both linked
    /// worktrees, the clean one is the only candidate, the first press only
    /// arms the button, and the second removes the clean worktree and leaves
    /// the one with an untracked file, and the branch, where they were.
    ///
    /// Mutation-tested: removing on the first press fails the "armed, not
    /// removed" assertion.
    #[gpui::test]
    fn the_worktree_review_removes_only_the_clean_one_and_only_on_a_second_press(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let dir = std::env::temp_dir().join(format!("td-wtview-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str], at: &Path| {
            let ok = std::process::Command::new("git")
                .args(args)
                .current_dir(at)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q", "-b", "main"], &repo);
        std::fs::write(repo.join("a"), "a").unwrap();
        git(&["add", "a"], &repo);
        git(&["commit", "-q", "-m", "a"], &repo);
        git(&["worktree", "add", "-q", "-b", "clean", "../clean"], &repo);
        git(&["worktree", "add", "-q", "-b", "messy", "../messy"], &repo);
        std::fs::write(dir.join("messy/new"), "x").unwrap();

        let (ws, cx) = book_window(cx);
        redraw(cx);
        let empty_home = dir.join("home");
        std::fs::create_dir_all(&empty_home).unwrap();
        ws.update(cx, |ws, cx| {
            ws.wt_review.fixture = Some((vec![repo.clone()], empty_home.clone()));
            ws.wt_prefs = wtclean::Prefs {
                unchanged: Some(true),
                ..Default::default()
            };
            ws.open_settings(Some(Page::Worktrees), cx);
        });
        let settled = |cx: &mut gpui::VisualTestContext| {
            for _ in 0..500 {
                redraw(cx);
                if ws.read_with(cx, |ws, _| !ws.wt_review.busy) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("the review never settled");
        };
        settled(cx);
        let found = ws.read_with(cx, |ws, _| match &ws.wt_review.got {
            Some(Ok((facts, repos))) => (facts.len(), *repos),
            other => panic!("no reading: {:?}", other.as_ref().map(|r| r.as_ref().err())),
        });
        assert_eq!(found, (3, 1), "the main checkout and two linked worktrees");
        let button = cx
            .debug_bounds("st-wt-remove")
            .expect("a candidate brings the remove button");
        cx.simulate_click(button.center(), gpui::Modifiers::none());
        redraw(cx);
        assert!(
            ws.read_with(cx, |ws, _| ws.wt_review.armed),
            "the first press arms"
        );
        assert!(dir.join("clean").exists(), "armed, not removed");
        let button = cx.debug_bounds("st-wt-remove").expect("still there, armed");
        cx.simulate_click(button.center(), gpui::Modifiers::none());
        settled(cx);
        assert!(!dir.join("clean").exists(), "the second press removes");
        assert!(
            dir.join("messy/new").exists(),
            "the untracked file's tree stays"
        );
        let said = ws.read_with(cx, |ws, _| ws.wt_review.said.clone().unwrap_or_default());
        assert!(said.starts_with('1'), "{said}");
        let branches = std::process::Command::new("git")
            .args(["branch", "--list", "clean"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&branches.stdout).contains("clean"),
            "the branch is kept"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Open shows the panel in place: the surface config draws inside the
    /// book's page and the book stays open, the first Esc goes back to the
    /// page, the second puts the book away; and the keys sheet opens in place
    /// the same way. Parker, 2026-11-09: *"Open SHOULD open it in place!"*
    ///
    /// Mutation-tested: closing the book in `st_open` again fails "the book
    /// stays open"; leaving `unhost` out of `settings_peel` fails "back to
    /// the page".
    #[gpui::test]
    fn open_shows_the_panel_in_place_and_esc_goes_back_to_the_page(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx) = book_window(cx);
        redraw(cx);
        ws.update(cx, |ws, cx| ws.open_settings(Some(Page::Appearance), cx));
        redraw(cx);
        let open = cx.debug_bounds("st-open-palette").expect("the Open button");
        cx.simulate_click(open.center(), gpui::Modifiers::none());
        redraw(cx);
        let state = |cx: &mut gpui::VisualTestContext| {
            ws.read_with(cx, |ws, _| {
                (
                    ws.settings.as_ref().map(|b| b.page),
                    ws.hosting(),
                    ws.theme_menu.is_some(),
                )
            })
        };
        assert_eq!(
            state(cx),
            (Some(Page::Appearance), Some(Host::Palette), true),
            "the book stays open, showing the surface config"
        );
        let page = cx.debug_bounds("st-hosted").expect("the page holds it");
        let tray = cx.debug_bounds("theme-panel").expect("the tray is drawn");
        assert!(
            page.contains(&tray.origin),
            "the tray is drawn on the page, not over the window: {tray:?} in {page:?}"
        );
        assert!(
            cx.debug_bounds("st-crumb-host").is_some(),
            "the breadcrumb names it"
        );
        cx.simulate_keystrokes("escape");
        redraw(cx);
        assert_eq!(
            state(cx),
            (Some(Page::Appearance), None, false),
            "Esc goes back to the page"
        );
        ws.update(cx, |ws, cx| ws.settings_go(Page::Keybindings, cx));
        redraw(cx);
        let open = cx
            .debug_bounds("st-open-keys")
            .expect("the keys row's Open");
        cx.simulate_click(open.center(), gpui::Modifiers::none());
        redraw(cx);
        assert_eq!(
            ws.read_with(cx, |ws, _| (ws.hosting(), ws.help_open)),
            (Some(Host::Keys), true),
            "the keys sheet opens in place too"
        );
        cx.simulate_keystrokes("escape");
        cx.simulate_keystrokes("escape");
        redraw(cx);
        assert_eq!(
            ws.read_with(cx, |ws, _| (ws.settings.is_some(), ws.help_open)),
            (false, false),
            "and the second Esc puts the book away"
        );
    }
}
