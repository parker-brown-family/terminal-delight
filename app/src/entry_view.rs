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

/// A draft put away: the text, the pictures pasted in, and the long pastes
/// its tokens stand for, each as (token, text).
pub(crate) type Draft = (String, Vec<std::path::PathBuf>, Vec<(String, String)>);

/// The composer's state while it is up.
pub(crate) struct Composer {
    /// What the person is asking for.
    pub(crate) text: EditBuffer,
    /// Pictures pasted in, saved under the session's surfaces; they go with
    /// the sentence as paths (see [`entry::with_pictures`]).
    pictures: Vec<std::path::PathBuf>,
    /// Long pastes, each as the token that stands for it in `text` and the
    /// text itself; the text takes the token's place when the prompt is sent
    /// (see [`entry::unfold`]).
    pub(crate) pastes: Vec<(String, String)>,
    pub(crate) focus: entry::Focus,
    /// What the scan found, with what was added from the composer
    /// (`projects.toml`) in front; a far one carries its machine.
    pub(crate) projects: Vec<launcher::Project>,
    /// Typed on the chip that has the keyboard: it narrows that chip's list.
    pub(crate) query: String,
    /// The chosen project, an index into `projects`. `None` is a choice of its
    /// own, *none, don't know yet*: the agent starts in a new folder of its
    /// own under `~/.scratch`, as a T3 "No project" thread does, and its tab is
    /// filed under no project.
    pub(crate) project: Option<usize>,
    pub(crate) harness: launcher::Harness,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) reach: launcher::Reach,
    /// This machine first, then any far ones from `machines.toml`.
    pub(crate) machines: Vec<entry::Machine>,
    pub(crate) machine: usize,
    machines_error: Option<String>,
    /// What `projects.toml` could not give: a read error, or a machine it
    /// names that `machines.toml` no longer lists.
    projects_note: Option<String>,
    /// The custom runner from `local-agent.toml`, read when the page opens —
    /// the LAUNCH AGENT panel's habit — so the preview reads no file a frame.
    local_adapter: Result<launcher::LocalAdapter, String>,
    /// The last thing the composer has to say: an error, or what it started
    /// in the background.
    pub(crate) note: Option<String>,
    /// The add-a-project dialog, while it is up over the composer
    /// (`addproject_view.rs`).
    pub(crate) adding: Option<addproject_view::AddProject>,
    /// Where an agent with no project gets its folder of its own
    /// ([`entry::scratch_root`]); a test points it somewhere of its own.
    pub(crate) scratch_root: std::path::PathBuf,
}

/// Project rows the PROJECT chip's list draws at once.
const PROJECT_ROWS: usize = 8;

/// Rows every other chip's list draws at once: one more than the project
/// list, in the place its add row takes, which has a row's measure. So every
/// list stands the same height, and the card, which is centred, does not move
/// while Tab walks the chips.
const CHOICE_ROWS: usize = PROJECT_ROWS + 1;

/// What a row in a chip's list sets. PROJECT's rows are indices into
/// `projects` and have a list of their own, with the add row and none.
#[derive(Clone, Debug, PartialEq)]
enum Pick {
    Harness(launcher::Harness),
    Model(Option<String>),
    Effort(Option<String>),
    Reach(launcher::Reach),
    Machine(usize),
}

impl Composer {
    fn project_path(&self) -> Option<&std::path::Path> {
        self.project
            .and_then(|i| self.projects.get(i))
            .map(|p| p.path.as_path())
    }

    /// Where the agent's project is, or home when there is none yet. A
    /// projectless agent's own folder is only made when it is sent, so that
    /// browsing the chips makes nothing on disk.
    fn cwd(&self) -> std::path::PathBuf {
        self.project_path()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(session::home_dir)
    }

    /// The chosen project's own machine, when it is on a far one.
    fn project_machine(&self) -> Option<&str> {
        self.project
            .and_then(|i| self.projects.get(i))
            .and_then(|p| p.machine.as_deref())
    }

    /// A far project takes its machine with it: choosing it chooses where it
    /// is. A project on this machine leaves the MACHINE chip as it was.
    fn follow_project(&mut self) {
        if let Some(name) = self.project_machine() {
            if let Some(i) = self.machines.iter().position(|m| m.name == name) {
                self.machine = i;
            }
        }
    }

    fn catalog(&self) -> model_catalog::Snapshot {
        self.harness.models(&self.cwd())
    }

    fn far(&self) -> bool {
        self.machines.len() > 1
    }

    /// Indices into `projects` that match what is typed, best first. Worked
    /// out each time it is read and never kept: a kept copy went stale when
    /// leaving the chip cleared the letters, and the list came back narrowed
    /// under an empty field (verify/composer, `ListMatchesField`).
    pub(crate) fn project_order(&self) -> Vec<usize> {
        launcher::filter(&self.projects, &self.query)
    }

    /// The PROJECT list's rows: none first, then the projects that match.
    /// None is there while nothing is typed, or when what is typed asks for
    /// it ("no", "none", its own word), as T3 finds its "No project" row.
    pub(crate) fn project_rows(&self, l: lang::Lang) -> Vec<Option<usize>> {
        let e = l.strings().entry;
        let none = self.query.trim().is_empty()
            || entry::asks_for_none(
                &self.query,
                &[e.project_none, e.project_none_chip, "no project", "none"],
            );
        none.then_some(None)
            .into_iter()
            .chain(self.project_order().into_iter().map(Some))
            .collect()
    }

    /// The options of the chip that has the keyboard, other than PROJECT's,
    /// in the order its arrows step them. A value the chip holds that the
    /// list does not offer (a model the catalog has not listed) is kept as a
    /// row of its own, so stepping away from it is a choice and not a loss;
    /// while the catalog is still being read, it says so rather than calling
    /// the value unavailable.
    fn choices(&self, l: lang::Lang) -> Vec<entry::Choice<Pick>> {
        let p = l.strings().parts;
        let row = |value, label: String, caption: String| entry::Choice {
            value,
            label,
            caption,
        };
        let missing = |catalog: &model_catalog::Snapshot, word: String| -> (String, String) {
            if catalog.ready {
                let label = lang::fill(p.model_choice_unavailable, &[("choice", &word)]);
                (label, String::new())
            } else {
                (word, catalog.status.text(l))
            }
        };
        match self.focus {
            entry::Focus::Text | entry::Focus::Project => Vec::new(),
            entry::Focus::Harness => launcher::Harness::ALL
                .into_iter()
                .map(|h| {
                    // What `local` runs, from local-agent.toml; the other two
                    // are their own commands.
                    let caption = match (&self.local_adapter, h) {
                        (Ok(adapter), launcher::Harness::Local) => adapter.command.join(" "),
                        _ => String::new(),
                    };
                    row(Pick::Harness(h), h.label_in(l).to_string(), caption)
                })
                .collect(),
            entry::Focus::Model => {
                let catalog = self.catalog();
                let mut rows: Vec<_> = std::iter::once(None)
                    .chain(catalog.models.iter().filter_map(|m| m.id.clone()).map(Some))
                    .map(|id| {
                        let model = catalog.model(id.as_deref());
                        let label = model
                            .map(|m| m.label_in(l))
                            .unwrap_or_else(|| model_catalog::Model::agent_default().label_in(l));
                        // The wire id, where the label is not already it.
                        let caption = model
                            .and_then(|m| m.resolved_id.clone().or_else(|| m.id.clone()))
                            .filter(|wire| !wire.eq_ignore_ascii_case(&label))
                            .unwrap_or_default();
                        row(Pick::Model(id), label, caption)
                    })
                    .collect();
                if let Some(id) = self
                    .model
                    .clone()
                    .filter(|id| catalog.model(Some(id)).is_none())
                {
                    let (label, caption) = missing(&catalog, id.clone());
                    rows.push(row(Pick::Model(Some(id)), label, caption));
                }
                rows
            }
            entry::Focus::Effort => {
                let catalog = self.catalog();
                // What the model does when it is not told, where it says.
                let own = catalog
                    .model(self.model.as_deref())
                    .and_then(|m| m.default_effort.as_deref())
                    .map(|e| model_catalog::effort_in(Some(e), l))
                    .unwrap_or_default();
                let offered = catalog.efforts(self.model.as_deref());
                let mut rows: Vec<_> = std::iter::once(None)
                    .chain(offered.iter().cloned().map(Some))
                    .map(|effort| {
                        let caption = if effort.is_none() {
                            own.clone()
                        } else {
                            String::new()
                        };
                        let label = model_catalog::effort_in(effort.as_deref(), l);
                        row(Pick::Effort(effort), label, caption)
                    })
                    .collect();
                if let Some(effort) = self.effort.clone().filter(|e| !offered.contains(e)) {
                    let word = model_catalog::effort_in(Some(&effort), l);
                    let (label, caption) = missing(&catalog, word);
                    rows.push(row(Pick::Effort(Some(effort)), label, caption));
                }
                rows
            }
            // The flags each one adds, which is what it means; the local
            // runner's own are local-agent.toml's, so it has only anywhere.
            entry::Focus::Reach => launcher::Reach::ALL
                .into_iter()
                .filter(|r| {
                    self.harness != launcher::Harness::Local || *r == launcher::Reach::Anywhere
                })
                .map(|r| {
                    row(
                        Pick::Reach(r),
                        r.label_in(l).to_string(),
                        r.flags(self.harness).join(" "),
                    )
                })
                .collect(),
            entry::Focus::Machine => self
                .machines
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let caption = m
                        .ssh
                        .as_ref()
                        .map(|dest| format!("ssh {dest}"))
                        .unwrap_or_default();
                    row(Pick::Machine(i), m.name.clone(), caption)
                })
                .collect(),
        }
    }

    /// The value of the chip that has the keyboard, as its list's rows hold
    /// it; `None` on the sentence and on PROJECT.
    fn current(&self) -> Option<Pick> {
        Some(match self.focus {
            entry::Focus::Text | entry::Focus::Project => return None,
            entry::Focus::Harness => Pick::Harness(self.harness),
            entry::Focus::Model => Pick::Model(self.model.clone()),
            entry::Focus::Effort => Pick::Effort(self.effort.clone()),
            entry::Focus::Reach => Pick::Reach(self.reach),
            entry::Focus::Machine => Pick::Machine(self.machine),
        })
    }

    /// Take a row of a chip's list. A harness of its own brings its own
    /// models, so a model or effort chosen for the last one goes; a model of
    /// its own may not take the effort chosen for the last.
    fn take(&mut self, pick: Pick) {
        match pick {
            Pick::Harness(h) => {
                if h != self.harness {
                    self.harness = h;
                    self.model = None;
                    self.effort = None;
                    if h == launcher::Harness::Local {
                        self.reach = launcher::Reach::Anywhere;
                    }
                }
            }
            Pick::Model(id) => {
                if id != self.model {
                    self.model = id;
                    self.effort = None;
                }
            }
            Pick::Effort(effort) => self.effort = effort,
            Pick::Reach(r) => self.reach = r,
            Pick::Machine(i) => self.machine = i,
        }
        self.note = None;
    }

    /// What was typed changed: narrow the list, and take its first match, as
    /// the project list always has. A query that matches nothing keeps the
    /// last choice: a typo is not a decision.
    fn refilter(&mut self, l: lang::Lang) {
        let typed = !self.query.trim().is_empty();
        if self.focus == entry::Focus::Project {
            if typed {
                if let Some(&first) = self.project_rows(l).first() {
                    self.project = first;
                    self.follow_project();
                }
            }
            return;
        }
        let all = self.choices(l);
        if typed {
            if let Some(&first) = entry::narrow(&all, &self.query).first() {
                self.take(all[first].value.clone());
            }
        }
    }

    /// Step the chip that has the keyboard, forward or back, through what
    /// its list shows.
    fn step_chip(&mut self, back: bool, l: lang::Lang) {
        match self.focus {
            entry::Focus::Text => {}
            entry::Focus::Project => {
                if let Some(next) = entry::cycle(&self.project_rows(l), &self.project, back) {
                    self.project = next;
                    self.follow_project();
                }
            }
            _ => {
                let all = self.choices(l);
                let shown: Vec<Pick> = entry::narrow(&all, &self.query)
                    .into_iter()
                    .map(|i| all[i].value.clone())
                    .collect();
                if let Some(next) = self
                    .current()
                    .and_then(|now| entry::cycle(&shown, &now, back))
                {
                    self.take(next);
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
            let scanned = launcher::scan(&launcher::default_roots(&home), 1000);
            let e = self.lang.strings().entry;
            let (projects, projects_note) = match addproject::load_projects(&config) {
                Ok(added) => {
                    let (projects, lost) =
                        addproject::with_added(scanned, &added, &machines, &home);
                    let note = lost
                        .first()
                        .map(|m| lang::fill(e.add_lost_machine, &[("machine", m)]));
                    (projects, note)
                }
                Err(err) => (
                    scanned,
                    Some(lang::fill(e.add_projects_error, &[("error", &err)])),
                ),
            };
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
                    .filter(|(_, p)| p.machine.is_none() && cwd.starts_with(&p.path))
                    .max_by_key(|(_, p)| p.path.as_os_str().len())
                    .map(|(i, _)| i)
            });
            // Standing in no project opens on none rather than on whichever
            // repository sorts first.
            let pref = &self.launch_defaults;
            let (draft, pictures, pastes) = self.composer_stash.take().unwrap_or_default();
            self.composer = Some(Composer {
                text: EditBuffer::seeded(&draft),
                pictures,
                pastes,
                focus: entry::Focus::Text,
                projects,
                query: String::new(),
                project,
                harness: pref.harness().unwrap_or(launcher::Harness::Claude),
                model: pref.model.clone().filter(|id| id != "configured"),
                effort: pref.effort.clone(),
                reach: launcher::Reach::Anywhere,
                machines,
                machine: 0,
                machines_error,
                projects_note,
                local_adapter: launcher::LocalAdapter::load(),
                note: None,
                adding: None,
                scratch_root: entry::scratch_root(&home),
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
            self.composer_stash = (!text.trim().is_empty() || !c.pictures.is_empty())
                .then_some((text, c.pictures, c.pastes));
        }
    }

    /// Esc, from the window's capture phase: off a chip and back to the
    /// sentence first, and only then is the page put away, its draft kept.
    /// No window here, and none needed: with no overlay up, the next frame
    /// hands the keyboard back to the active pane (`plan_focus`).
    pub(crate) fn composer_escape(&mut self) {
        if self.composer.as_ref().is_some_and(|c| c.adding.is_some()) {
            self.add_project_escape();
            return;
        }
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
        if self.composer.as_ref().is_some_and(|c| c.adding.is_some()) {
            return self.add_project_key(ks, window, cx);
        }
        if ks.key == "o" && m.control && !m.alt && !m.shift {
            self.add_project_open(cx);
            return true;
        }
        let Some(mut c) = self.composer.take() else {
            return false;
        };
        let far = c.far();
        let l = self.lang;
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
            // Enter on a chip takes what its list has lit and goes back to the
            // sentence, as T3's pickers do; Ctrl+Enter still starts from there.
            "enter" if !m.control && c.focus != entry::Focus::Text => {
                c.focus = entry::Focus::Text;
                c.query.clear();
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
                            if entry::folds(&text) {
                                // A long paste goes in as its token; the text
                                // itself waits for the send.
                                let lines = text.lines().count().max(1);
                                let token_for = |n: usize| {
                                    lang::fill(
                                        self.lang.strings().entry.composer_paste,
                                        &[("n", &n.to_string()), ("lines", &lines.to_string())],
                                    )
                                };
                                // Numbered past every paste still held, so no
                                // two tokens read alike after one is deleted.
                                let mut n = c.pastes.len() + 1;
                                while c.pastes.iter().any(|(t, _)| *t == token_for(n)) {
                                    n += 1;
                                }
                                let token = token_for(n);
                                c.text.insert_capped(&token, 20_000);
                                c.pastes.push((token, text));
                            } else {
                                c.text.insert_capped(&text, 20_000);
                            }
                        }
                    }
                }
                c.note = None;
            }
            key => match c.focus {
                entry::Focus::Text => {
                    // Backspace right after a paste's token takes the whole
                    // token, and the paste with it.
                    let gone = (key == "backspace" && !m.control)
                        .then(|| {
                            c.pastes
                                .iter()
                                .position(|(token, _)| c.text.delete_back_run(token))
                        })
                        .flatten();
                    match gone {
                        Some(i) => {
                            c.pastes.remove(i);
                        }
                        None => c.text.apply(key, m, ks.key_char.as_deref(), 20_000),
                    }
                    c.note = None;
                }
                // Every chip's list the same way: the arrows step through it,
                // typing narrows it and takes its first match.
                _ => match key {
                    "left" | "up" => c.step_chip(true, l),
                    "right" | "down" => c.step_chip(false, l),
                    // With nothing typed, Backspace on PROJECT takes the
                    // project away: one key to an agent with no project.
                    "backspace" if c.query.is_empty() => {
                        if c.focus == entry::Focus::Project {
                            c.project = None;
                        }
                    }
                    "backspace" => {
                        c.query.pop();
                        c.refilter(l);
                    }
                    _ => {
                        let typed = ks.key_char.as_deref().filter(|_| !m.control && !m.alt);
                        // A space before any word narrows nothing.
                        if let Some(ch) =
                            typed.filter(|ch| !(c.query.is_empty() && ch.trim().is_empty()))
                        {
                            c.query.push_str(ch);
                            c.refilter(l);
                        }
                    }
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
        // What was typed, with each long paste back in its token's place.
        let sentence = entry::unfold(&c.text.text(), &c.pastes);
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
        // A far project's folder is on its own machine: started anywhere else,
        // the agent would begin in a folder that is not there.
        let far_project = c.project_machine().map(str::to_string);
        if let Some(home_of) = &far_project {
            if *home_of != machine.name {
                let name = c
                    .project
                    .and_then(|i| c.projects.get(i))
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                c.note = Some(lang::fill(
                    e.add_elsewhere,
                    &[("project", &name), ("machine", home_of)],
                ));
                self.composer = Some(c);
                cx.notify();
                return;
            }
        }
        // No project: a folder of its own, named from the date and the first
        // words, the way T3 starts a "No project" thread. Here it is claimed
        // now; on a far machine the ssh line makes it (`line_on`'s `make`).
        let mut cwd = cwd;
        let mut claimed: Option<std::path::PathBuf> = None;
        if c.project.is_none() {
            let date = crate::handover::local(surfacefeed::now_ms())
                .map(|t| format!("{:04}-{:02}-{:02}", t.year, t.month, t.day));
            let name = entry::scratch_name(&sentence, date.as_deref());
            if machine.is_far() {
                cwd = entry::scratch_root(&session::home_dir()).join(name);
            } else {
                match entry::claim_scratch(&c.scratch_root, &name) {
                    Ok(dir) => {
                        cwd = dir.clone();
                        claimed = Some(dir);
                    }
                    Err(err) => {
                        c.note = Some(lang::fill(
                            e.composer_scratch_failed,
                            &[("error", &err.to_string())],
                        ));
                        self.composer = Some(c);
                        cx.notify();
                        return;
                    }
                }
            }
        }
        // The pane that types the ssh line is this machine's, so it starts in
        // a folder this machine has.
        let pane_cwd = if far_project.is_some() || (machine.is_far() && c.project.is_none()) {
            session::home_dir()
        } else {
            cwd.clone()
        };
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
                // Nothing started in it, so the folder claimed for it goes too
                // (`remove_dir` takes only an empty one).
                if let Some(dir) = &claimed {
                    let _ = std::fs::remove_dir(dir);
                }
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
            self.composer_seat(&cwd, far_project.is_some(), cx)
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
            Some(pane_cwd.to_string_lossy().to_string()),
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
                c.project.is_none(),
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
    ///
    /// A far project's folder is on another machine, so no terminal here is
    /// standing in it, whatever path the two share: its tab is filed by name
    /// only (Codex's review, round one).
    fn composer_seat(&mut self, cwd: &std::path::Path, far: bool, cx: &mut Context<Self>) -> Seat {
        for (ti, tab) in self.tabs.iter().enumerate().filter(|_| !far) {
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
                        .text_color(sk.ink.ink_faint)
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
                            .text_color(sk.ink.ink_faint)
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
                            .text_color(sk.ink.ink_faint)
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
        if c.adding.is_some() {
            let card = self.render_add_project(th, &sk, s, cx)?;
            return Some(Self::composer_page(th, s, card));
        }
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
            .unwrap_or_else(|| e.project_none_chip.into());
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
            let shown = if on && !c.query.is_empty() {
                format!("{value}  \u{2039}{}\u{203a}", c.query)
            } else {
                value
            };
            chip_row = chip_row.child(
                sk.control_chip(on)
                    .id(SharedString::from(format!("composer-chip-{focus:?}")))
                    .debug_selector(move || format!("composer-chip-{focus:?}"))
                    .cursor_pointer()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(5. * s))
                    .child(
                        // The chip's own ink, dimmed: a lit chip in some skins
                        // is a solid fill (hacker's is bright green), and any
                        // fixed colour vanished on one state or the other.
                        div()
                            .text_size(sk.text_px(8.5 * s))
                            .opacity(0.62)
                            .child(header),
                    )
                    .child(div().text_size(sk.text_px(11. * s)).child(shown))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if let Some(c) = ws.composer.as_mut() {
                                // A chip opens its list; pressed again, it puts
                                // the list away, as a dropdown does.
                                c.focus = if c.focus == focus {
                                    entry::Focus::Text
                                } else {
                                    focus
                                };
                                c.query.clear();
                            }
                            window.focus(&ws.focus_handle, cx);
                            cx.notify();
                        }),
                    ),
            );
            // Adding a project sits beside the chip that chooses one, drawn as
            // a chip so it wears the row's own edge in every skin (a bezel
            // button had a border where the skin's chips have none).
            if focus == entry::Focus::Project {
                chip_row = chip_row.child(
                    sk.chip(false)
                        .id("composer-add-project")
                        .debug_selector(|| "composer-add-project".into())
                        .cursor_pointer()
                        .text_size(sk.text_px(11. * s))
                        .text_color(th.accent)
                        // The skin's own word for a control (terminimalist
                        // spells controls out), as a bezel button would.
                        .child(sk.control_text(e.add_button))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                ws.add_project_open(cx);
                                window.focus(&ws.focus_handle, cx);
                            }),
                        ),
                );
            }
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
        // While a list is open the chips' values change under the keys, and a
        // value long enough to wrap the row onto a second line, or short enough
        // to unwrap it, moved the centred card a line at a step. So the row
        // stands at least two chip lines tall while a chip has the keyboard:
        // two empty chips share its grid cell, and a cell is as tall as the
        // taller of what is in it. The chips keep to the top of the cell at
        // their own height; stretched to it, a lit chip's bar ran two lines.
        let chip_row = if typing {
            chip_row.into_any_element()
        } else {
            let ghost = || {
                sk.control_chip(false)
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(5. * s))
                    .child(div().text_size(sk.text_px(8.5 * s)).child("\u{a0}"))
                    .child(div().text_size(sk.text_px(11. * s)).child("\u{a0}"))
            };
            div()
                .grid()
                .grid_cols(1)
                .child(
                    div()
                        .col_start(1)
                        .row_start(1)
                        .flex()
                        .flex_col()
                        .gap(px(6. * s))
                        .opacity(0.)
                        .child(ghost())
                        .child(ghost()),
                )
                .child(chip_row.col_start(1).row_start(1).self_start())
                .into_any_element()
        };

        // The list of the chip that has the keyboard, under the chips:
        // PROJECT's is the launcher's ranking of what was typed, with the add
        // row and none; every other chip lists its own options.
        let list = match c.focus {
            entry::Focus::Text => None,
            entry::Focus::Project => Some(self.composer_project_list(c, th, &sk, s, cx)),
            _ => Some(self.composer_choice_list(c, th, &sk, s, cx)),
        };

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
                                .text_color(sk.ink.ink_faint)
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
        let notes: Vec<String> = [
            c.note.clone(),
            c.machines_error.clone(),
            c.projects_note.clone(),
            far_note,
        ]
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
                    .text_color(sk.ink.ink_dim)
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
                        sk.ink.edge_rest
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
                    .text_color(sk.ink.ink_faint)
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
                    .text_color(sk.ink.ink_faint)
                    // While a chip has the keyboard, its list's keys: Enter
                    // there goes back to the sentence rather than starting.
                    .child(if typing {
                        e.composer_hint
                    } else {
                        e.composer_list_hint
                    }),
            );

        Some(Self::composer_page(th, s, column))
    }

    /// PROJECT's list: the add row, a status line, then none and the
    /// projects, held at eight rows whatever is typed.
    fn composer_project_list(
        &self,
        c: &Composer,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let e = self.lang.strings().entry;
        let ui = self.lang.strings().ui;
        let mut list = div().flex().flex_col().gap(px(1. * s));
        list = list.child(
            div()
                .id("composer-project-add")
                .debug_selector(|| "composer-project-add".into())
                .px(px(8. * s))
                .py(px(2. * s))
                .rounded(sk.radius())
                .cursor_pointer()
                .hover(|st| st.bg(hsla(0., 0., 1., 0.06)))
                .text_size(sk.text_px(11.5 * s))
                .text_color(th.accent)
                .child(e.add_row)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        ws.add_project_open(cx);
                        window.focus(&ws.focus_handle, cx);
                    }),
                ),
        );
        let rows = c.project_rows(self.lang);
        list = list.child(Self::composer_status(
            sk,
            s,
            if c.query.trim().is_empty() {
                e.composer_filter
            } else if rows.is_empty() {
                ui.la_no_match
            } else {
                "\u{a0}"
            },
        ));
        // None leads whatever the window shows, as T3's "No project" does:
        // as the first row of a window kept on the chosen project, it went
        // out of sight whenever that project was the ninth or later.
        let pinned = usize::from(rows.first() == Some(&None));
        let slots = PROJECT_ROWS - pinned;
        // The rest a window at a time, kept on the chosen project: a list
        // that always opened at its top hid the choice the chip names
        // whenever it was the ninth repository or later.
        let chosen = rows
            .iter()
            .skip(pinned)
            .position(|r| *r == c.project)
            .unwrap_or(0);
        let start = pinned + chosen.saturating_sub(slots - 1);
        let below = rows.len().saturating_sub(start + slots);
        let window = rows.iter().copied().enumerate().skip(start).take(slots);
        let mut drawn = 0;
        for (row, i) in rows.iter().copied().enumerate().take(pinned).chain(window) {
            let (name, path, key) = match i {
                // ⌫ is the keyboard's way here while nothing is typed.
                None => (
                    e.project_none.to_string(),
                    e.project_none_where.to_string(),
                    c.query.is_empty().then_some("\u{232b}"),
                ),
                Some(at) => {
                    let Some(p) = c.projects.get(at) else {
                        continue;
                    };
                    let path = match &p.machine {
                        Some(m) => format!("{m} \u{00b7} {}", p.path.to_string_lossy()),
                        None => p.path.to_string_lossy().to_string(),
                    };
                    (p.name.clone(), path, None)
                }
            };
            let on = c.project == i;
            let id: SharedString = match i {
                None => "composer-project-none".into(),
                Some(_) => format!("composer-project-{row}").into(),
            };
            list = list.child(
                Self::composer_row(th, sk, s, id, name, path, key, on).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        if let Some(c) = ws.composer.as_mut() {
                            c.project = i;
                            c.follow_project();
                            c.query.clear();
                            c.focus = entry::Focus::Text;
                        }
                        window.focus(&ws.focus_handle, cx);
                        cx.notify();
                    }),
                ),
            );
            drawn += 1;
        }
        // Held at eight rows whatever is typed: the card is centred, so a
        // list that shrank under a filter moved the sentence box up the
        // screen with every letter.
        for _ in drawn..PROJECT_ROWS {
            list = list.child(Self::composer_blank(sk, s));
        }
        list.child(Self::composer_more(sk, s, e, below))
    }

    /// Any other chip's list: a status line, then its options, held at nine
    /// rows so it stands as tall as PROJECT's. The chip's value is lit; the
    /// arrows step it, typing narrows the list, a press takes a row.
    fn composer_choice_list(
        &self,
        c: &Composer,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let e = self.lang.strings().entry;
        let ui = self.lang.strings().ui;
        let all = c.choices(self.lang);
        let shown = entry::narrow(&all, &c.query);
        let current = c.current();
        let mut list = div().flex().flex_col().gap(px(1. * s));
        list = list.child(Self::composer_status(
            sk,
            s,
            if c.query.trim().is_empty() {
                e.composer_narrow
            } else if shown.is_empty() {
                ui.la_no_match
            } else {
                "\u{a0}"
            },
        ));
        let chosen = shown
            .iter()
            .position(|&i| Some(&all[i].value) == current.as_ref())
            .unwrap_or(0);
        let start = chosen.saturating_sub(CHOICE_ROWS - 1);
        let below = shown.len().saturating_sub(start + CHOICE_ROWS);
        let mut drawn = 0;
        for (n, &i) in shown.iter().enumerate().skip(start).take(CHOICE_ROWS) {
            let choice = &all[i];
            let on = Some(&choice.value) == current.as_ref();
            let pick = choice.value.clone();
            list = list.child(
                Self::composer_row(
                    th,
                    sk,
                    s,
                    format!("composer-choice-{n}").into(),
                    choice.label.clone(),
                    choice.caption.clone(),
                    None,
                    on,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        if let Some(c) = ws.composer.as_mut() {
                            c.take(pick.clone());
                            c.query.clear();
                            c.focus = entry::Focus::Text;
                        }
                        window.focus(&ws.focus_handle, cx);
                        cx.notify();
                    }),
                ),
            );
            drawn += 1;
        }
        for _ in drawn..CHOICE_ROWS {
            list = list.child(Self::composer_blank(sk, s));
        }
        list.child(Self::composer_more(sk, s, e, below))
    }

    /// A row of a chip's list: its name, a quieter word beside it, and a key
    /// at its end, lit when it is the chip's value. Both lists draw their rows
    /// here, so a row has one measure in all of them.
    #[allow(clippy::too_many_arguments)]
    fn composer_row(
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        id: SharedString,
        label: String,
        caption: String,
        key: Option<&'static str>,
        on: bool,
    ) -> gpui::Stateful<gpui::Div> {
        let selector = id.to_string();
        // Faint is unreadable on the lit row's wash.
        let quiet = if on { sk.ink.ink_dim } else { sk.ink.ink_faint };
        div()
            .id(id)
            .debug_selector(move || selector.clone())
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
                    .flex_none()
                    .whitespace_nowrap()
                    .text_size(sk.text_px(11.5 * s))
                    .text_color(if on { th.text } else { th.text.alpha(0.8) })
                    .child(label),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_size(sk.text_px(9.5 * s))
                    .text_color(quiet)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(caption),
            )
            .children(key.map(|k| {
                div()
                    .flex_none()
                    .text_size(sk.text_px(9.5 * s))
                    .text_color(quiet)
                    .child(k)
            }))
    }

    /// A list row's measure with nothing in it, so a short list stands as
    /// tall as a full one.
    fn composer_blank(sk: &skin::Skin, s: f32) -> gpui::Div {
        div()
            .px(px(8. * s))
            .py(px(2. * s))
            .flex()
            .flex_row()
            .gap(px(8. * s))
            .child(div().text_size(sk.text_px(11.5 * s)).child("\u{a0}"))
            .child(div().text_size(sk.text_px(9.5 * s)).child("\u{a0}"))
    }

    /// The line over a list's rows, always drawn: how to look, or that
    /// nothing matched.
    fn composer_status(sk: &skin::Skin, s: f32, said: &'static str) -> gpui::Div {
        div()
            .text_size(sk.text_px(10. * s))
            .text_color(sk.ink.ink_faint)
            .child(said)
    }

    /// The count under a list, always drawn so the list holds its height.
    fn composer_more(sk: &skin::Skin, s: f32, e: &lang::Entry, below: usize) -> gpui::Div {
        div()
            .px(px(8. * s))
            .text_size(sk.text_px(9.5 * s))
            .text_color(sk.ink.ink_faint)
            .child(if below > 0 {
                lang::fill(e.add_more, &[("n", &below.to_string())])
            } else {
                "\u{a0}".to_string()
            })
    }

    /// The page the composer's card, or the add-a-project dialog's, stands
    /// on: over the panes, owning the pointer.
    fn composer_page(th: &theme::Theme, s: f32, card: impl IntoElement) -> AnyElement {
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
            .child(card)
            .into_any_element()
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

    /// The T3 scuttle's surfaces draw secondary text, hints and edges in the
    /// skin's inks (`ink_faint`, `ink_dim`, `edge_rest`, `rule`), which every
    /// skin derives from the palette's text and can restyle, never in the
    /// palette's raw `faint`: in the hacker design that colour is #14401f on
    /// black, and the composer's paths and hints vanished (found by the skin
    /// sweep, 2026-11-09). The side panel already did; these now do too.
    #[test]
    fn the_t3_surfaces_draw_secondary_text_in_the_skins_inks() {
        for (file, src) in [
            ("entry_view.rs", include_str!("entry_view.rs")),
            ("addproject_view.rs", include_str!("addproject_view.rs")),
            ("relate_view.rs", include_str!("relate_view.rs")),
            ("inbox_view.rs", include_str!("inbox_view.rs")),
            ("flyout_view.rs", include_str!("flyout_view.rs")),
        ] {
            let body = src.split("#[cfg(test)]").next().unwrap_or(src);
            let raw = ["th", ".faint"].concat();
            assert!(
                !body.contains(&raw),
                "{file} draws with the palette's raw faint"
            );
        }
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
