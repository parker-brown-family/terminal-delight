//! The composer, drawn: a sentence first, then where it runs and with what.
//!
//! The model is `entry.rs`; the launch is the launcher's. This file holds the
//! composer's state, its keys, the page it draws over the main area, and the
//! send — which is [`Workspace::adopt_pane`] into a new tab, the same funnel
//! LAUNCH AGENT and `ctl adopt` use, so a pane started from a sentence gets a
//! host pane id, survives this window, and is deduplicated like any other.
//!
//! It sits OVER the panes (inside the screen, absolutely placed), so opening
//! it resizes no terminal and stops none: *"we will have all the TERMINALs
//! running, but what it feels like is an ENTRY point"*. That makes it a panel
//! over the glass, which is why `composer` is in the warp suppression list and
//! in [`OVERLAYS_OVER_PANES`].

use super::*;

/// The composer's state while it is up.
pub(crate) struct Composer {
    /// What the person is asking for.
    pub(crate) text: EditBuffer,
    /// Pictures pasted in, saved under the session's surfaces; they go with
    /// the sentence as paths (see [`entry::with_pictures`]).
    pictures: Vec<std::path::PathBuf>,
    pub(crate) focus: entry::Focus,
    projects: Vec<launcher::Project>,
    /// Typed on the PROJECT chip.
    query: String,
    /// Indices into `projects` that match `query`, best first.
    order: Vec<usize>,
    /// The chosen project, an index into `projects`. `None` is a choice of its
    /// own, *none, don't know yet*: the agent starts in the home folder and
    /// its tab is filed under no project.
    project: Option<usize>,
    harness: launcher::Harness,
    model: Option<String>,
    effort: Option<String>,
    reach: launcher::Reach,
    /// This machine first, then any far ones from `machines.toml`.
    machines: Vec<entry::Machine>,
    machine: usize,
    machines_error: Option<String>,
    /// The custom runner from `local-agent.toml`, read when the page opens —
    /// the LAUNCH AGENT panel's habit — so the preview reads no file a frame.
    local_adapter: Result<launcher::LocalAdapter, String>,
    /// The last thing the composer has to say: an error, or what it started
    /// in the background.
    note: Option<String>,
}

impl Composer {
    fn project_path(&self) -> Option<&std::path::Path> {
        self.project
            .and_then(|i| self.projects.get(i))
            .map(|p| p.path.as_path())
    }

    /// Where the agent starts: the project, or home when there is none yet.
    fn cwd(&self) -> std::path::PathBuf {
        self.project_path()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(session::home_dir)
    }

    fn catalog(&self) -> model_catalog::Snapshot {
        self.harness.models(&self.cwd())
    }

    fn far(&self) -> bool {
        self.machines.len() > 1
    }

    fn refilter(&mut self) {
        self.order = launcher::filter(&self.projects, &self.query);
        // A query that matches nothing keeps the last choice rather than
        // falling to none: a typo is not a decision to have no project.
        if !self.query.is_empty() {
            if let Some(&first) = self.order.first() {
                self.project = Some(first);
            }
        }
    }

    /// Step the chip that has the keyboard, forward or back.
    fn step_chip(&mut self, back: bool) {
        match self.focus {
            entry::Focus::Text => {}
            entry::Focus::Project => {
                // None leads the list until something is typed.
                let list: Vec<Option<usize>> = self
                    .query
                    .is_empty()
                    .then_some(None)
                    .into_iter()
                    .chain(self.order.iter().copied().map(Some))
                    .collect();
                if let Some(next) = entry::cycle(&list, &self.project, back) {
                    self.project = next;
                }
            }
            entry::Focus::Harness => {
                if let Some(next) = entry::cycle(&launcher::Harness::ALL, &self.harness, back) {
                    self.harness = next;
                    // Model lists differ by harness; a model or effort valid for
                    // one may not exist in the next.
                    self.model = None;
                    self.effort = None;
                    if next == launcher::Harness::Local {
                        self.reach = launcher::Reach::Anywhere;
                    }
                }
            }
            entry::Focus::Model => {
                let ids: Vec<Option<String>> = std::iter::once(None)
                    .chain(
                        self.catalog()
                            .models
                            .iter()
                            .filter_map(|m| m.id.clone())
                            .map(Some),
                    )
                    .collect();
                if let Some(next) = entry::cycle(&ids, &self.model, back) {
                    self.model = next;
                    self.effort = None;
                }
            }
            entry::Focus::Effort => {
                let efforts: Vec<Option<String>> = std::iter::once(None)
                    .chain(
                        self.catalog()
                            .efforts(self.model.as_deref())
                            .into_iter()
                            .map(Some),
                    )
                    .collect();
                if let Some(next) = entry::cycle(&efforts, &self.effort, back) {
                    self.effort = next;
                }
            }
            entry::Focus::Reach => {
                if self.harness != launcher::Harness::Local {
                    if let Some(next) = entry::cycle(&launcher::Reach::ALL, &self.reach, back) {
                        self.reach = next;
                    }
                }
            }
            entry::Focus::Machine => {
                let all: Vec<usize> = (0..self.machines.len()).collect();
                if let Some(next) = entry::cycle(&all, &self.machine, back) {
                    self.machine = next;
                }
            }
        }
        self.note = None;
    }
}

impl Workspace {
    /// Open the composer over the main area, or bring the keyboard back to it.
    ///
    /// The chips open on what somebody chose for LAUNCH AGENT (the launch
    /// defaults on the usage card), and the project on the repository the
    /// person is standing in, by path — a name can match two directories.
    pub(crate) fn open_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The queue answers keys in the capture phase; it folds when the page
        // opens over it, through its own door so its lifecycle runs.
        if self.rail_open {
            self.rail_close();
        }
        if self.composer.is_none() {
            let home = session::home_dir();
            let projects = launcher::scan(&launcher::default_roots(&home), 1000);
            let here = self
                .tabs
                .get(self.active)
                .and_then(|t| t.focused)
                .and_then(|id| self.pane_by_id(id))
                .and_then(|p| p.read(cx).current_cwd())
                .map(std::path::PathBuf::from);
            // The longest project path the cwd sits in: standing in
            // `~/Work/td/app/src` is standing in `~/Work/td`, not in `~/Work`.
            let project = here.as_ref().and_then(|cwd| {
                projects
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| cwd.starts_with(&p.path))
                    .max_by_key(|(_, p)| p.path.as_os_str().len())
                    .map(|(i, _)| i)
            });
            // Standing in no project opens on none rather than on whichever
            // repository sorts first.
            let order = launcher::filter(&projects, "");
            let pref = &self.launch_defaults;
            let config = instance::config_dir();
            let (far, machines_error) = match entry::load_machines(&config) {
                Ok(far) => (far, None),
                Err(e) => (
                    Vec::new(),
                    Some(lang::fill(
                        self.lang.strings().entry.composer_machines_error,
                        &[("error", &e)],
                    )),
                ),
            };
            let host = std::fs::read_to_string("/proc/sys/kernel/hostname").ok();
            let mut machines = vec![entry::Machine::here(
                host.as_deref(),
                self.lang.strings().entry.composer_machine,
            )];
            machines.extend(far);
            let (draft, pictures) = self.composer_stash.take().unwrap_or_default();
            self.composer = Some(Composer {
                text: EditBuffer::seeded(&draft),
                pictures,
                focus: entry::Focus::Text,
                projects,
                query: String::new(),
                order,
                project,
                harness: pref.harness().unwrap_or(launcher::Harness::Claude),
                model: pref.model.clone().filter(|id| id != "configured"),
                effort: pref.effort.clone(),
                reach: launcher::Reach::Anywhere,
                machines,
                machine: 0,
                machines_error,
                local_adapter: launcher::LocalAdapter::load(),
                note: None,
            });
            self.watch_model_catalogs(cx);
        }
        // Take the keyboard off the focused pane, whose key handler writes to
        // the PTY, exactly as every picker does.
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    /// Put the composer away. What was typed is kept for the next time it
    /// opens, the way T3 keeps a draft: a sentence half written is work.
    pub(crate) fn close_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer_put_away();
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Take the page down with its draft kept. Needs no window: with no
    /// overlay up, the next frame hands the keyboard back to the active pane
    /// (`plan_focus`).
    pub(crate) fn composer_put_away(&mut self) {
        if let Some(c) = self.composer.take() {
            let text = c.text.text();
            self.composer_stash =
                (!text.trim().is_empty() || !c.pictures.is_empty()).then_some((text, c.pictures));
        }
    }

    /// Esc, from the window's capture phase: off a chip and back to the
    /// sentence first, and only then is the page put away, its draft kept.
    /// No window here, and none needed: with no overlay up, the next frame
    /// hands the keyboard back to the active pane (`plan_focus`).
    pub(crate) fn composer_escape(&mut self) {
        let Some(mut c) = self.composer.take() else {
            return;
        };
        if c.focus != entry::Focus::Text {
            c.focus = entry::Focus::Text;
            c.query.clear();
            self.composer = Some(c);
            return;
        }
        self.composer = Some(c);
        self.composer_put_away();
    }

    /// The composer's keys. Returns whether it took the key — always, while it
    /// is up: a key that fell through would reach a terminal behind a page.
    pub(crate) fn composer_key(
        &mut self,
        ks: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let m = &ks.modifiers;
        // The two entry chords still work with the page up: E puts it away,
        // I turns the spine between its faces.
        if m.control && m.shift && !m.alt {
            match ks.key.as_str() {
                "e" => {
                    self.close_composer(window, cx);
                    return true;
                }
                "i" => {
                    let showing = self.spine_inbox();
                    self.set_spine_inbox(!showing, cx);
                    return true;
                }
                _ => {}
            }
        }
        let Some(mut c) = self.composer.take() else {
            return false;
        };
        let far = c.far();
        match ks.key.as_str() {
            "escape" => {
                if c.focus != entry::Focus::Text {
                    c.focus = entry::Focus::Text;
                    c.query.clear();
                    self.composer = Some(c);
                    cx.notify();
                } else {
                    self.composer = Some(c);
                    self.close_composer(window, cx);
                }
                return true;
            }
            "enter" if m.shift && c.focus == entry::Focus::Text => {
                c.text.insert("\n");
            }
            "enter" => {
                self.composer = Some(c);
                let send = if m.control {
                    entry::Send::Background
                } else {
                    entry::Send::Open
                };
                self.composer_send(send, window, cx);
                return true;
            }
            "tab" => {
                c.focus = c.focus.step(far, m.shift);
                c.query.clear();
            }
            "v" if m.control && c.focus == entry::Focus::Text => {
                match Self::composer_clipboard_picture(cx) {
                    Some(path) => {
                        if !c.pictures.contains(&path) {
                            c.pictures.push(path);
                        }
                    }
                    None => {
                        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                            c.text.insert_capped(&text, 20_000);
                        }
                    }
                }
                c.note = None;
            }
            key => match c.focus {
                entry::Focus::Text => {
                    c.text.apply(key, m, ks.key_char.as_deref(), 20_000);
                    c.note = None;
                }
                entry::Focus::Project => match key {
                    "left" | "up" => c.step_chip(true),
                    "right" | "down" => c.step_chip(false),
                    "backspace" => {
                        c.query.pop();
                        c.refilter();
                    }
                    _ => {
                        if let Some(ch) = ks.key_char.as_deref().filter(|_| !m.control && !m.alt) {
                            c.query.push_str(ch);
                            c.refilter();
                        }
                    }
                },
                _ => match key {
                    "left" | "up" => c.step_chip(true),
                    "right" | "down" | "space" => c.step_chip(false),
                    _ => {}
                },
            },
        }
        self.composer = Some(c);
        cx.notify();
        true
    }

    /// Start what the composer describes.
    ///
    /// The checks are the launcher's own (a model the catalog does not offer,
    /// an effort that model does not take) and the launch is too; what is new
    /// is only the sentence as the agent's first message, the tab named after
    /// it, and the far machine.
    pub(crate) fn composer_send(
        &mut self,
        send: entry::Send,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut c) = self.composer.take() else {
            return;
        };
        let e = self.lang.strings().entry;
        let sentence = c.text.text();
        if sentence.trim().is_empty() && c.pictures.is_empty() {
            c.note = Some(e.composer_placeholder.to_string());
            self.composer = Some(c);
            cx.notify();
            return;
        }
        let cwd = c.cwd();
        let filed = c.project.is_some();
        let machine = c
            .machines
            .get(c.machine)
            .cloned()
            .unwrap_or_else(|| entry::Machine::here(None, e.composer_machine));
        let catalog = c.catalog();
        let parts = self.lang.strings().parts;
        if c.harness != launcher::Harness::Local {
            if c.model.is_some() && (!catalog.ready || catalog.model(c.model.as_deref()).is_none())
            {
                c.note = Some(lang::fill(
                    parts.model_selected_unavailable,
                    &[("status", &catalog.status.text(self.lang))],
                ));
                self.composer = Some(c);
                cx.notify();
                return;
            }
            if let Some(effort) = &c.effort {
                if !catalog.efforts(c.model.as_deref()).contains(effort) {
                    c.note = Some(parts.model_selected_effort.to_string());
                    self.composer = Some(c);
                    cx.notify();
                    return;
                }
            }
        }
        let recipe = entry::recipe(
            c.harness,
            c.model.clone(),
            c.effort.clone(),
            c.reach,
            cwd.clone(),
            &entry::with_pictures(&sentence, &c.pictures),
        );
        let line = match Self::composer_line(&c, &machine, &recipe) {
            Ok(line) => line,
            Err(err) => {
                c.note = Some(err);
                self.composer = Some(c);
                cx.notify();
                return;
            }
        };
        eprintln!("terminal-delight: composer launching — {line}");
        if let Some(dir) = surfacefeed::session_dir() {
            let _ = surfacefeed::journal_event(
                &dir.join("launches.jsonl"),
                &serde_json::json!({
                    "td": crate::surface::TDSP_VERSION,
                    "type": "launch",
                    "via": "composer",
                    "at_ms": surfacefeed::now_ms(),
                    "harness": recipe.harness.binary(),
                    "model": recipe.model,
                    "effort": recipe.effort,
                    "reach": recipe.reach.id(),
                    "cwd": cwd.to_string_lossy(),
                    "machine": machine.name,
                    "command": line,
                    "landing": match send {
                        entry::Send::Open => "new-tab",
                        entry::Send::Background => "new-tab-behind",
                    },
                }),
            );
        }
        let seat = if filed {
            self.composer_seat(&cwd, cx)
        } else {
            Seat::Loose
        };
        let before = self.active;
        // Put the composer back BEFORE the tab opens when it is staying, so the
        // new tab's deferred focus sees an overlay holding the keyboard and
        // leaves it there (see `open_tab`).
        let name = entry::tab_name(&sentence, 28);
        let stays = send == entry::Send::Background;
        if stays {
            c.text = EditBuffer::seeded("");
            c.pictures.clear();
            c.focus = entry::Focus::Text;
            c.note = Some(lang::fill(
                e.composer_started,
                &[("name", name.as_deref().unwrap_or(""))],
            ));
            self.composer = Some(c);
        } else {
            self.composer_stash = None;
        }
        self.adopt_pane(
            Some(cwd.to_string_lossy().to_string()),
            Some(line),
            seat,
            window,
            cx,
        );
        let opened = self.active;
        if let Some(tab) = self.tabs.get_mut(opened) {
            if tab.name.is_none() {
                tab.name = name;
            }
        }
        if stays {
            // Behind: the person stays where they were, and the new tab waits
            // in its branch. `open_tab` inserted it, which moved every tab at
            // or after its slot one to the right.
            self.active = if opened <= before { before + 1 } else { before };
            window.focus(&self.focus_handle, cx);
        }
        self.save(cx);
        cx.notify();
    }

    /// A picture on the clipboard, saved under this session's surfaces, or
    /// `None` when the clipboard holds none. The compositor's clipboard first
    /// (a screenshot tool's paste), then gpui's, the two routes a pane's paste
    /// takes.
    fn composer_clipboard_picture(cx: &mut Context<Self>) -> Option<std::path::PathBuf> {
        let dir = surfacefeed::session_dir()?.join("composer").join("pastes");
        if let Some((bytes, ext)) = crate::workbench::clipboard_image() {
            return crate::workbench::write_paste(&dir, &bytes, ext).map(Into::into);
        }
        let item = cx.read_from_clipboard()?;
        item.entries().iter().find_map(|entry| match entry {
            gpui::ClipboardEntry::Image(image) => crate::workbench::write_paste(
                &dir,
                &image.bytes,
                crate::workbench::ext_of_image_format(image.format),
            )
            .map(Into::into),
            _ => None,
        })
    }

    /// The line a send types.
    ///
    /// A far agent is handed no briefing: the briefing is a file on this
    /// machine and the drop directory it names is this window's, so neither
    /// means anything over there. A custom local runner is this machine's by
    /// definition.
    fn composer_line(
        c: &Composer,
        machine: &entry::Machine,
        recipe: &launcher::Recipe,
    ) -> Result<String, String> {
        if machine.is_far() {
            if c.harness == launcher::Harness::Local {
                return Err(lang::fill(
                    lang::current().strings().entry.composer_far_local,
                    &[("machine", &machine.name)],
                ));
            }
            return Ok(entry::line_on(
                machine,
                &session::home_dir(),
                &recipe.cwd,
                &recipe.command_line(None),
            ));
        }
        let dir = surfacefeed::session_dir().ok_or("TD could not find its session directory")?;
        let briefing = recipe.prepare_briefing(&dir, skills::briefing_root().as_deref())?;
        let adapter = if c.harness == launcher::Harness::Local {
            Some(c.local_adapter.as_ref().map_err(|e| e.clone())?)
        } else {
            None
        };
        recipe.command_line_with_adapter(Some(&briefing), adapter)
    }

    /// Where a composed agent's tab goes: beside the filed terminals already
    /// open in that repository, else under the left-bar project of the same
    /// name, else under a new project named the way *adopt projects from
    /// folders* names one. Never in whatever branch the person happens to be
    /// standing in: a project was chosen, so the tab belongs to it, and on a
    /// fresh window that branch is no branch at all.
    fn composer_seat(&mut self, cwd: &std::path::Path, cx: &mut Context<Self>) -> Seat {
        for (ti, tab) in self.tabs.iter().enumerate() {
            if self.place_of(ti).unfiled() {
                continue;
            }
            let mut leaves = Vec::new();
            tab.root.leaves(&mut leaves);
            let inside = leaves.iter().any(|leaf| {
                leaf.read(cx)
                    .current_cwd()
                    .is_some_and(|d| std::path::Path::new(&d).starts_with(cwd))
            });
            if inside {
                return Seat::Branch(self.place_of(ti));
            }
        }
        let projects: Vec<(u32, String)> =
            self.projects.iter().map(|p| (p.id, p.label())).collect();
        let id = match entry::project_by_name(cwd, &projects) {
            Some(id) => id,
            None => {
                let id = self.new_project(None, cx);
                if let Some(p) = self.projects.iter_mut().find(|p| p.id == id) {
                    p.name = project_name_for(&cwd.to_string_lossy());
                }
                id
            }
        };
        Seat::Branch(tree::Place {
            project: Some(id),
            initiative: None,
        })
    }

    /// Open a far machine's screen in gliff.
    fn composer_screen(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.composer.as_mut() else {
            return;
        };
        let Some(cmd) = c.machines.get(c.machine).and_then(entry::screen_command) else {
            return;
        };
        let args: Vec<&str> = cmd[1..].iter().map(String::as_str).collect();
        pane::launch_gui(&cmd[0], &args);
        cx.notify();
    }

    /// Under the chips, what is already waiting: LOOK AT ME's first five rows,
    /// each one click from its pane. The page is the front door both ways —
    /// start something new, or go to what wants you — the way T3's home lists
    /// the threads that came back beside its composer.
    fn composer_waiting(
        &self,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        let (shelves, _) = self.inbox_rows(cx);
        if shelves.look.is_empty() {
            return None;
        }
        let e = self.lang.strings().entry;
        let mut list = div().flex().flex_col().gap(px(2. * s)).child(
            div()
                .flex()
                .flex_row()
                .gap(px(6. * s))
                .text_size(sk.text_px(9.5 * s))
                .child(div().text_color(sk.ink.warn).child(e.shelf_look))
                .child(
                    div()
                        .text_color(th.faint)
                        .child(shelves.wanting().to_string()),
                ),
        );
        for (n, row) in shelves.look.iter().take(5).enumerate() {
            let key = row.fact.key;
            let pane = row.fact.pane;
            let age = attention::age_label_in(
                self.lang,
                row.fact
                    .changed
                    .map(|c| Instant::now().saturating_duration_since(c)),
            );
            list = list.child(
                div()
                    .id(SharedString::from(format!("composer-waiting-{n}")))
                    .debug_selector(move || format!("composer-waiting-{n}"))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8. * s))
                    .px(px(6. * s))
                    .py(px(2. * s))
                    .rounded(sk.radius())
                    .cursor_pointer()
                    .hover(|st| st.bg(hsla(0., 0., 1., 0.06)))
                    .child(
                        div()
                            .flex_none()
                            .text_size(sk.text_px(11.5 * s))
                            .text_color(th.text)
                            .child(row.fact.title.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(sk.text_px(9.5 * s))
                            .text_color(th.faint)
                            .child(
                                row.fact
                                    .line
                                    .clone()
                                    .unwrap_or_else(|| row.fact.place.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(sk.text_px(9.5 * s))
                            .text_color(th.faint)
                            .child(age),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.composer_put_away();
                            ws.inbox_go(key, pane, cx);
                        }),
                    ),
            );
        }
        Some(list)
    }

    /// The composer, over the main area, or nothing.
    pub(crate) fn render_composer(
        &self,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let c = self.composer.as_ref()?;
        let s = theme::outer_choice(cx).grade.scale;
        let sk = skin::skin(cx, s);
        let e = self.lang.strings().entry;
        let ui = self.lang.strings().ui;
        let catalog = c.catalog();

        // The draft, a run of text per line with the caret riding in it as a
        // highlight — the bench composer's and the note box's way, which keeps
        // the caret right on a wrapped line.
        let text = c.text.text();
        let caret_style = gpui::HighlightStyle {
            background_color: Some(th.cursor),
            color: Some(th.bg),
            ..Default::default()
        };
        let typing = c.focus == entry::Focus::Text;
        let mut draft = div().flex().flex_col().min_h(px(84. * s));
        if text.is_empty() {
            draft = draft.child(
                div()
                    .text_color(th.text.alpha(0.38))
                    .child(e.composer_placeholder),
            );
        }
        let caret_at = c.text.caret();
        let mut start = 0usize;
        for line in text.split('\n') {
            let len = line.chars().count();
            let row = div().w_full().min_h(px(th.font_size * 1.35 * s));
            let row = match caret_at
                .checked_sub(start)
                .filter(|at| *at <= len && typing)
            {
                Some(at) => {
                    let (shown, span) = docview::notes_ui::caret_span(line, at);
                    row.child(
                        gpui::StyledText::new(shown).with_highlights(vec![(span, caret_style)]),
                    )
                }
                None => row.child(line.to_string()),
            };
            draft = draft.child(row);
            start += len + 1;
        }

        let project_name = c
            .project
            .and_then(|i| c.projects.get(i))
            .map(|p| p.name.clone())
            .unwrap_or_else(|| e.project_none.into());
        let model_name = c
            .model
            .as_deref()
            .and_then(|id| catalog.model(Some(id)))
            .map(|m| m.label_in(self.lang))
            .or_else(|| c.model.clone())
            .unwrap_or_else(|| model_catalog::Model::agent_default().label_in(self.lang));
        let machine = c.machines.get(c.machine);
        let chips: Vec<(entry::Focus, &str, String)> = {
            let mut v = vec![
                (entry::Focus::Project, ui.la_project, project_name),
                (
                    entry::Focus::Harness,
                    ui.la_harness,
                    c.harness.label_in(self.lang).to_string(),
                ),
                (entry::Focus::Model, ui.la_model, model_name),
                (
                    entry::Focus::Effort,
                    ui.la_effort,
                    model_catalog::effort_in(c.effort.as_deref(), self.lang),
                ),
                (
                    entry::Focus::Reach,
                    ui.la_reach,
                    c.reach.label_in(self.lang).to_string(),
                ),
            ];
            if c.far() {
                v.push((
                    entry::Focus::Machine,
                    e.composer_machine,
                    machine.map(|m| m.name.clone()).unwrap_or_default(),
                ));
            }
            v
        };
        let mut chip_row = div().flex().flex_row().flex_wrap().gap(px(6. * s));
        for (focus, header, value) in chips {
            let on = c.focus == focus;
            let shown = if on && focus == entry::Focus::Project && !c.query.is_empty() {
                format!("{value}  \u{2039}{}\u{203a}", c.query)
            } else {
                value
            };
            chip_row = chip_row.child(
                sk.chip(on)
                    .id(SharedString::from(format!("composer-chip-{focus:?}")))
                    .debug_selector(move || format!("composer-chip-{focus:?}"))
                    .cursor_pointer()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(5. * s))
                    .child(
                        div()
                            .text_size(sk.text_px(8.5 * s))
                            .text_color(th.faint)
                            .child(header),
                    )
                    .child(div().text_size(sk.text_px(11. * s)).child(shown))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, ev: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if let Some(c) = ws.composer.as_mut() {
                                if c.focus == focus && focus != entry::Focus::Project {
                                    c.step_chip(ev.modifiers.shift);
                                } else {
                                    c.focus = focus;
                                    c.query.clear();
                                }
                            }
                            window.focus(&ws.focus_handle, cx);
                            cx.notify();
                        }),
                    ),
            );
        }
        if machine.is_some_and(entry::Machine::is_far) {
            chip_row = chip_row.child(
                Self::bezel_btn_s(&sk, e.composer_screen, false, s * 0.9)
                    .id("composer-screen")
                    .debug_selector(|| "composer-screen".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.composer_screen(cx);
                        }),
                    ),
            );
        }

        // The PROJECT chip's list, while it has the keyboard: the launcher's
        // ranking of what was typed, eight at a time.
        let list = (c.focus == entry::Focus::Project).then(|| {
            let mut list = div()
                .flex()
                .flex_col()
                .gap(px(1. * s))
                .max_h(px(8. * 24. * s));
            if c.order.is_empty() {
                list = list.child(
                    div()
                        .text_size(sk.text_px(10.5 * s))
                        .text_color(th.faint)
                        .child(ui.la_no_match),
                );
            }
            if c.query.is_empty() {
                list = list.child(
                    div()
                        .text_size(sk.text_px(10. * s))
                        .text_color(th.faint)
                        .child(e.composer_filter),
                );
            }
            // None leads the list until something is typed, as it leads the
            // chip's arrow keys.
            let rows: Vec<Option<usize>> = c
                .query
                .is_empty()
                .then_some(None)
                .into_iter()
                .chain(c.order.iter().copied().map(Some))
                .take(8)
                .collect();
            for (row, i) in rows.into_iter().enumerate() {
                let (name, path) = match i.and_then(|i| c.projects.get(i)) {
                    Some(p) => (p.name.clone(), p.path.to_string_lossy().to_string()),
                    None => (e.project_none.to_string(), String::new()),
                };
                let on = c.project == i;
                list = list.child(
                    div()
                        .id(SharedString::from(format!("composer-project-{row}")))
                        .debug_selector(move || format!("composer-project-{row}"))
                        .px(px(8. * s))
                        .py(px(2. * s))
                        .rounded(sk.radius())
                        .cursor_pointer()
                        .when(on, |d| d.bg(th.accent.alpha(0.22)))
                        .hover(|st| st.bg(hsla(0., 0., 1., 0.06)))
                        .flex()
                        .flex_row()
                        .gap(px(8. * s))
                        .child(
                            div()
                                .text_size(sk.text_px(11.5 * s))
                                .text_color(if on { th.text } else { th.text.alpha(0.8) })
                                .child(name),
                        )
                        .child(
                            div()
                                .text_size(sk.text_px(9.5 * s))
                                .text_color(th.faint)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(path),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                if let Some(c) = ws.composer.as_mut() {
                                    c.project = i;
                                    c.query.clear();
                                    c.focus = entry::Focus::Text;
                                }
                                window.focus(&ws.focus_handle, cx);
                                cx.notify();
                            }),
                        ),
                );
            }
            list
        });

        // Pasted pictures, by name, each with its own way out. A name rather
        // than a thumbnail: a pasted picture is an attachment, and the agent is
        // the one that looks at it.
        let pictures = (!c.pictures.is_empty()).then(|| {
            let mut row = div().flex().flex_row().flex_wrap().gap(px(6. * s));
            for (n, path) in c.pictures.iter().enumerate() {
                let name = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();
                row = row.child(
                    sk.chip(false)
                        .id(SharedString::from(format!("composer-picture-{n}")))
                        .debug_selector(move || format!("composer-picture-{n}"))
                        .cursor_pointer()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6. * s))
                        .child(
                            div()
                                .text_size(sk.text_px(11. * s))
                                .child(format!("\u{25a3} {name}")),
                        )
                        .child(
                            div()
                                .text_size(sk.text_px(10. * s))
                                .text_color(th.faint)
                                .child("\u{2715}"),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                if let Some(c) = ws.composer.as_mut() {
                                    if n < c.pictures.len() {
                                        c.pictures.remove(n);
                                    }
                                }
                                window.focus(&ws.focus_handle, cx);
                                cx.notify();
                            }),
                        ),
                );
            }
            row
        });
        let far_note = machine
            .filter(|m| m.is_far())
            .map(|m| lang::fill(e.composer_far_note, &[("machine", &m.name)]));
        let notes: Vec<String> = [c.note.clone(), c.machines_error.clone(), far_note]
            .into_iter()
            .flatten()
            .collect();
        let local_note = (c.harness == launcher::Harness::Local).then_some(ui.la_local_note);

        let column = div()
            .id("composer-column")
            .w_full()
            .max_w(px(760. * s))
            .flex()
            .flex_col()
            .gap(px(12. * s))
            .p(px(22. * s))
            .map(|d| dialog_card(th, &sk, d))
            // A click inside the page is the page's: it must not reach the
            // panes behind it, and it gives the keyboard back to the page.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.focus(&ws.focus_handle, cx);
                }),
            )
            .child(
                div()
                    .text_size(sk.text_px(22. * s))
                    .text_color(th.text)
                    .child(e.composer_title),
            )
            .child(
                div()
                    .text_size(sk.text_px(11. * s))
                    .text_color(th.faint)
                    .child(e.composer_sub),
            )
            .child(
                div()
                    .id("composer-text")
                    .debug_selector(|| "composer-text".into())
                    .p(px(12. * s))
                    .rounded(sk.capped(10.))
                    .bg(darken(th.bg, 0.4).alpha(0.85))
                    .border(sk.box_border(1.))
                    .border_color(if typing {
                        th.accent.alpha(0.6)
                    } else {
                        th.faint.alpha(0.3)
                    })
                    .text_size(sk.text_px(14. * s))
                    .text_color(th.text)
                    .cursor_text()
                    .child(draft)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if let Some(c) = ws.composer.as_mut() {
                                c.focus = entry::Focus::Text;
                            }
                            window.focus(&ws.focus_handle, cx);
                            cx.notify();
                        }),
                    ),
            )
            .children(pictures)
            .child(chip_row)
            .children(list)
            .children(local_note.map(|n| {
                div()
                    .text_size(sk.text_px(10. * s))
                    .text_color(th.faint)
                    .child(n)
            }))
            .children(notes.into_iter().map(|n| {
                div()
                    .text_size(sk.text_px(10.5 * s))
                    .text_color(th.accent)
                    .child(n)
            }))
            .children(self.composer_waiting(th, &sk, s, cx))
            .child(
                div()
                    .text_size(sk.text_px(9.5 * s))
                    .text_color(th.faint)
                    .child(e.composer_hint),
            );

        Some(
            div()
                // The card stands over the panes, so the warp's grade is kept
                // off it, the way every floating card in the house is.
                .on_children_prepainted(keep_panels_from_doc_grade)
                .id("composer-page")
                .debug_selector(|| "composer-page".into())
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .p(px(24. * s))
                .bg(th.bg.alpha(0.72))
                // The page owns the pointer over the whole screen: nothing
                // behind it is clicked through it.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(column)
                .into_any_element(),
        )
    }
}

/// The frame a centred dialog over the panes wears: the house's floating
/// card, as the token-savings card has it. A two-pixel accent edge, an opaque
/// ground, the floating shadows, then the skin's own finish, so a dialog reads
/// as a box with an edge in every skin. The composer and the relate dialog
/// share it; before they did, each drew a faint edge of its own over a
/// see-through ground and the dialog had no visible end.
pub(crate) fn dialog_card<E>(th: &theme::Theme, sk: &skin::Skin, d: E) -> E
where
    E: Styled + ParentElement + gpui::prelude::FluentBuilder,
{
    d.relative()
        .overflow_hidden()
        .rounded_md()
        .border(sk.box_border(2.))
        .border_color(th.accent.alpha(0.85))
        .bg(darken(th.surface, 0.62))
        .shadow(float_shadows(th.accent, th.grade.phosphor))
        .map(|d| sk.popup(sk.finish(d)))
        .map(|d| sk.chrome(d))
}

#[cfg(test)]
mod tests {
    /// The composer is drawn INSIDE the screen — over the panes, in the same
    /// box they lay out in — and never in the row that sizes them, so opening
    /// it cannot resize a terminal.
    #[test]
    fn the_composer_is_over_the_screen_and_not_in_the_row_that_sizes_the_panes() {
        let src = include_str!("main.rs");
        let at = src.find("let screen = div()").expect("the screen");
        let end = src[at..].find("let stage = div()").expect("the stage");
        assert!(
            src[at..at + end].contains("self.render_composer("),
            "the composer is laid over the screen"
        );
        let stage =
            &src[at + end..at + end + src[at + end..].find("let root = div()").expect("root")];
        assert!(!stage.contains("render_composer"), "and not beside it");
    }

    /// The attention queue answers plain keys in the capture phase, ahead of
    /// the composer. With the page up it must answer none of them — a `y` in
    /// a sentence would approve an agent's access ask — and opening the page
    /// folds the queue (found by the round-one review from Codex).
    #[test]
    fn the_queue_takes_no_key_while_the_composer_is_up() {
        let src = include_str!("main.rs");
        let at = src
            .find("&& (ws.rail_key(key, cx) || ws.art_key(key, cx))")
            .expect("the queue's capture-phase keys");
        let guard = &src[at.saturating_sub(400)..at];
        assert!(
            guard.contains("ws.composer.is_none()"),
            "the queue's keys are not fenced off"
        );
        let view = include_str!("entry_view.rs");
        let open = view.find("fn open_composer(").expect("open_composer");
        assert!(
            view[open..open + 600].contains("self.rail_close()"),
            "opening the page does not fold the queue"
        );
    }

    /// A pane counts as visited only while somebody can be looking at it.
    #[test]
    fn a_pane_behind_a_page_is_not_visited() {
        let src = include_str!("main.rs");
        assert!(src
            .contains("let looking = window.is_window_active() && !self.overlay_owns_keyboard();"));
        let view = include_str!("inbox_view.rs");
        let tick = view.find("fn inbox_tick(").expect("inbox_tick");
        assert!(view[tick..tick + 900].contains(".filter(|_| looking)"));
    }

    /// While it is up the composer holds the keyboard, and every gate that
    /// asks whether an overlay does must be told.
    #[test]
    fn the_composer_holds_the_keyboard_and_flattens_the_glass() {
        let src = include_str!("main.rs");
        for (gate, needle) in [
            ("fn overlay_owns_keyboard(", "self.composer.is_some()"),
            ("warp::set_suppressed(", "self.composer.is_some()"),
            ("fn close_popups(&mut self) -> bool {", "self.composer"),
        ] {
            let at = src.find(gate).expect(gate);
            let body = &src[at..at + 4000.min(src.len() - at)];
            assert!(body.contains(needle), "{gate} does not know the composer");
        }
    }
}
