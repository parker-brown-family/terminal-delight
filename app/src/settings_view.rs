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
use crate::settings::{self, Book, Heading, Item, Page};

/// What an Open button does once the book has closed.
type Then = fn(&mut Workspace, &mut Window, &mut Context<Workspace>);
/// What a switch does when pressed.
type Flip = fn(&mut Workspace, &mut Context<Workspace>);

/// The nav page's width, as T3's sidebar is narrower than its page.
const NAV_W: f32 = 232.;

impl Workspace {
    /// Open the book at `page`, or at the page it was last on.
    pub(crate) fn open_settings(&mut self, page: Option<Page>, cx: &mut Context<Self>) {
        let page = page.or(self.settings_last).unwrap_or(Page::General);
        self.settings = Some(Book::at(page));
        self.settings_last = Some(page);
        cx.notify();
    }

    pub(crate) fn close_settings(&mut self, cx: &mut Context<Self>) {
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
        if let Some(b) = self.settings.as_mut() {
            b.page = page;
            b.query.clear();
            b.pick = 0;
        }
        self.settings_last = Some(page);
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
                self.close_settings(cx);
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

    /// The button on a row whose setting still opens in a panel of its own:
    /// the book closes and the panel opens.
    fn st_open(
        &self,
        th: &theme::Theme,
        sk: &skin::Skin,
        id: &'static str,
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
                    ws.close_settings(cx);
                    then(ws, w, cx);
                    cx.notify();
                }),
            )
            .into_any_element()
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
                        self.st_open(th, sk, "st-open-keys", |ws, _, _| ws.help_open = true, cx),
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
                            |ws, _, cx| ws.fetch_savings(None, cx),
                            cx,
                        ),
                    ),
                    row(
                        s.r_bug,
                        s.r_bug_sub,
                        self.st_open(th, sk, "st-open-bug", |ws, _, _| ws.open_bug_menu(), cx),
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
                .child(SharedString::from(label.to_string()))
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
        let w = 1080_f32.min((vw - 48.).max(320.));
        let h = 760_f32.min((vh - 72.).max(300.));
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
                            .text_color(th.text)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(SharedString::from(self.page_name(b.page).to_string())),
                    ),
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
            .children(self.settings_page(b.page, th, &sk, &sk1, cx));
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
            .child(body);
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
}
