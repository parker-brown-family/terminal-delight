//! ⚙ CONFIG › UPDATES: what this window runs, what its channel offers, and
//! the way back, in a centred panel in the style of ADMIN. The engine is
//! [`crate::update`]; this file only decides what to show and which button
//! does what.
//!
//! Three promises the panel keeps, from `docs/plans/update-channels/`:
//! nothing installs before a press; a press that would end terminals says so
//! with the number and asks twice; and what a window restart loses is named
//! before the press, not discovered after.
use super::*;
use crate::uninstall::roots::Roots;
use crate::update::{self, BuildInfo, Channel, Check, Compat, Install, Installed, Prepared};

#[derive(Default)]
pub(crate) struct UpdateState {
    choices: update::Choices,
    record: update::Record,
    install: Option<Install>,
    busy: bool,
    check: Option<Result<Check, String>>,
    /// A checked download whose wire differs, waiting for a choice.
    prepared: Option<Prepared>,
    /// What the last install or rollback did: the panel then offers Restart.
    installed: Option<Installed>,
    /// Whether that install keeps the session host.
    installed_compat: Option<Compat>,
    error: Option<String>,
    armed_full: bool,
    armed_older: bool,
    /// Other TD sessions with a live host, counted when the panel opens. A
    /// full restart refuses while there are any: their next window would meet
    /// a host speaking the old wire.
    others: usize,
}

impl Workspace {
    pub(crate) fn open_updates(&mut self, cx: &mut Context<Self>) {
        self.update_menu = true;
        let u = &mut self.update;
        u.armed_full = false;
        u.armed_older = false;
        u.error = None;
        if let Ok(roots) = Roots::current() {
            u.choices = update::load_choices(&roots);
            u.record = update::load_record(&roots);
            let exe = std::env::current_exe().unwrap_or_default();
            let image = std::env::var_os("APPIMAGE").map(std::path::PathBuf::from);
            u.install = Some(update::detect(&roots, &exe, image.as_deref()));
        }
        self.update.others =
            update::live_hosts().saturating_sub(usize::from(self.attach.is_some()));
        cx.notify();
    }

    pub(crate) fn close_updates(&mut self) {
        self.update_menu = false;
        self.update.armed_full = false;
        self.update.armed_older = false;
    }

    /// Terminals in this window: what a full restart would end.
    fn update_pane_count(&self) -> usize {
        self.tabs
            .iter()
            .map(|t| {
                let mut v = vec![];
                t.root.leaves(&mut v);
                v.len()
            })
            .sum()
    }

    fn update_set_channel(&mut self, channel: Channel, cx: &mut Context<Self>) {
        let Ok(roots) = Roots::current() else { return };
        let mut c = self.update.choices.clone();
        c.channel = channel;
        match update::save_choices(&roots, &c) {
            Ok(()) => {
                self.update.choices = c;
                self.update.check = None;
                self.update.prepared = None;
                self.update.armed_older = false;
                self.update_check(cx);
            }
            Err(e) => self.update.error = Some(e),
        }
        cx.notify();
    }

    fn update_toggle_daily(&mut self, cx: &mut Context<Self>) {
        let Ok(roots) = Roots::current() else { return };
        let mut c = self.update.choices.clone();
        c.check_daily = Some(c.check_daily != Some(true));
        match update::save_choices(&roots, &c) {
            Ok(()) => self.update.choices = c,
            Err(e) => self.update.error = Some(e),
        }
        cx.notify();
    }

    fn update_check(&mut self, cx: &mut Context<Self>) {
        if self.update.busy {
            return;
        }
        self.update.busy = true;
        self.update.error = None;
        let channel = self.update.choices.channel;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move {
                    let r = update::check(channel, &BuildInfo::this());
                    if r.is_ok() {
                        if let Ok(roots) = Roots::current() {
                            let mut rec = update::load_record(&roots);
                            rec.last_check = Some(update::now_unix());
                            let _ = update::save_record(&roots, &rec);
                        }
                    }
                    r
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.update.busy = false;
                if let Ok(roots) = Roots::current() {
                    ws.update.record = update::load_record(&roots);
                }
                ws.update.check = Some(done);
                cx.notify();
            });
        })
        .detach();
    }

    /// Download, check and — when the wire matches — install the offer. A
    /// build whose wire differs is staged and the choice is put to the person.
    fn update_install(&mut self, cx: &mut Context<Self>) {
        let Some(Ok(Check {
            offer: Some(offer), ..
        })) = &self.update.check
        else {
            return;
        };
        let Some(install) = self.update.install.clone() else {
            return;
        };
        if self.update.busy {
            return;
        }
        let offer = offer.clone();
        self.update.busy = true;
        self.update.error = None;
        self.update.armed_older = false;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move {
                    let roots = Roots::current()?;
                    let this_build = BuildInfo::this();
                    let p = update::prepare(&roots, &offer, &this_build)?;
                    if p.compat == Compat::Same {
                        let done = update::install(&roots, &install, &p, &this_build, false)?;
                        let _ = std::fs::remove_dir_all(
                            p.image
                                .parent()
                                .unwrap_or(std::path::Path::new("/nonexistent")),
                        );
                        Ok::<_, String>((Some(done), None))
                    } else {
                        update::stage(&roots, &p)?;
                        Ok((None, Some(p)))
                    }
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.update.busy = false;
                match done {
                    Ok((Some(done), _)) => {
                        ws.update.installed = Some(done);
                        ws.update.installed_compat = Some(Compat::Same);
                    }
                    Ok((None, Some(p))) => ws.update.prepared = Some(p),
                    Ok((None, None)) => {}
                    Err(e) => ws.update.error = Some(e),
                }
                if let Ok(roots) = Roots::current() {
                    ws.update.record = update::load_record(&roots);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn update_rollback(&mut self, cx: &mut Context<Self>) {
        let Some(install) = self.update.install.clone() else {
            return;
        };
        let Some(kept) = self.update.record.previous.clone() else {
            return;
        };
        if self.update.busy {
            return;
        }
        let this_build = BuildInfo::this();
        let compat = update::rollback_compat(&this_build, &kept);
        self.update.busy = true;
        self.update.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { update::rollback(&Roots::current()?, &install, &this_build) })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.update.busy = false;
                match done {
                    Ok(done) => {
                        ws.update.installed = Some(done);
                        ws.update.installed_compat = Some(compat);
                    }
                    Err(e) => ws.update.error = Some(e),
                }
                if let Ok(roots) = Roots::current() {
                    ws.update.record = update::load_record(&roots);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Start the new build as this session's window and quit this one. The
    /// session host and every terminal in it carry on; the new window attaches
    /// to them as a relaunch always has.
    fn update_restart_window(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self
            .update
            .installed
            .as_ref()
            .map(|i| i.entry.clone())
            .or_else(|| self.update.install.as_ref().and_then(Install::entry))
        else {
            return;
        };
        match update::relaunch_after_exit(&entry, instance::key()) {
            Ok(()) => cx.quit(),
            Err(e) => {
                self.update.error = Some(e);
                cx.notify();
            }
        }
    }

    /// Install a staged or just-installed build whose wire differs, stop the
    /// session host — which ends its terminals, as the button says — and
    /// start the new build in a fresh window. Asks twice. Refuses while other
    /// TD sessions run, because their next window would meet an old host.
    fn update_restart_everything(&mut self, cx: &mut Context<Self>) {
        if !self.update.armed_full {
            self.update.armed_full = true;
            cx.notify();
            return;
        }
        self.update.armed_full = false;
        let others = update::live_hosts().saturating_sub(usize::from(self.attach.is_some()));
        if others > 0 {
            self.update.error = Some(format!(
                "{others} other Terminal Delight sessions are running"
            ));
            cx.notify();
            return;
        }
        let roots = match Roots::current() {
            Ok(r) => r,
            Err(e) => {
                self.update.error = Some(e);
                cx.notify();
                return;
            }
        };
        let entry = if let Some(i) = &self.update.installed {
            Some(i.entry.clone())
        } else {
            let exe = std::env::current_exe().unwrap_or_default();
            update::apply_pending_at_cold_start(&roots, &exe, 0)
        };
        let Some(entry) = entry else {
            self.update.error =
                Some("the staged build could not be installed; see the window log".into());
            cx.notify();
            return;
        };
        if let Some(attach) = &self.attach {
            if let Err(e) = attach.link.shutdown() {
                self.update.error = Some(format!("the session host did not stop: {e}"));
                cx.notify();
                return;
            }
        }
        match update::relaunch_after_exit(&entry, instance::key()) {
            Ok(()) => cx.quit(),
            Err(e) => {
                self.update.error = Some(e);
                cx.notify();
            }
        }
    }

    /// The opt-in daily check, run from the window's timer: at most once a
    /// day, and it only ever raises a desktop notification, once per build.
    pub(crate) fn update_daily_tick(&mut self, cx: &mut Context<Self>) {
        let Ok(roots) = Roots::current() else { return };
        let choices = update::load_choices(&roots);
        if !update::daily_due(&choices, &update::load_record(&roots), update::now_unix()) {
            return;
        }
        let ui = self.lang.strings().ui;
        let (title, body) = (ui.upd_notify_title, ui.upd_notify_body);
        cx.background_executor()
            .spawn(async move {
                let found = update::check(choices.channel, &BuildInfo::this());
                let mut rec = update::load_record(&roots);
                rec.last_check = Some(update::now_unix());
                if let Ok(Check { offer: Some(o), .. }) = &found {
                    let key = format!("{}:{}", o.channel.word(), o.describe());
                    if o.is_update() && rec.last_announced.as_deref() != Some(&key) {
                        let _ = std::process::Command::new("notify-send")
                            .args(notify::notify_args(
                                &title.replace("{version}", &o.describe()),
                                body,
                            ))
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .spawn();
                        rec.last_announced = Some(key);
                    }
                }
                let _ = update::save_record(&roots, &rec);
            })
            .detach();
    }

    pub(crate) fn render_update_overlay(
        &self,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        if !self.update_menu {
            return None;
        }
        let ui = self.lang.strings().ui;
        let sk = skin::skin(cx, th.grade.scale);
        let u = &self.update;
        let me = BuildInfo::this();
        let panes = self.update_pane_count();
        let held = self.trash.live_len(Instant::now());

        let row =
            |glyph: &str, name: String, blurb: String, live: bool, warn: bool, id: &'static str| {
                div()
                    .id(SharedString::from(id))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .px_2()
                    .py_2()
                    .rounded(sk.radius())
                    .border(sk.box_border(1.))
                    .border_color(if warn {
                        gpui::hsla(0.0, 0.62, 0.52, 0.9)
                    } else if live {
                        th.accent.alpha(0.55)
                    } else {
                        th.text.alpha(0.12)
                    })
                    .when(live, |d| {
                        d.cursor_pointer().hover(|s| s.bg(th.accent.alpha(0.15)))
                    })
                    .map(|d| sk.action(d, false))
                    .when(!sk.text_controls(), |d| {
                        d.child(
                            div()
                                .text_size(sk.text_px(15.))
                                .child(SharedString::from(glyph.to_string())),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .flex_1()
                            .min_w(px(0.))
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                    .text_color(if live { th.text } else { th.text.alpha(0.45) })
                                    .child(SharedString::from(name)),
                            )
                            .child(
                                div()
                                    .text_size(sk.text_px(8.5))
                                    .text_color(if warn {
                                        gpui::hsla(0.0, 0.7, 0.68, 1.0)
                                    } else {
                                        th.text.alpha(if live { 0.6 } else { 0.35 })
                                    })
                                    .child(SharedString::from(blurb)),
                            ),
                    )
            };
        let note = |text: String, bad: bool| {
            div()
                .text_size(sk.text_px(8.))
                .text_color(if bad {
                    gpui::hsla(0.07, 0.9, 0.6, 1.0)
                } else {
                    th.text.alpha(0.6)
                })
                .child(SharedString::from(text))
        };
        let fact = |label: &'static str, value: String| {
            div()
                .flex()
                .flex_row()
                .gap_2()
                .text_size(sk.text_px(8.5))
                .child(
                    div()
                        .w(px(110.))
                        .flex_none()
                        .text_color(th.text.alpha(0.5))
                        .child(label),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .child(SharedString::from(value)),
                )
        };
        let chip = |label: &'static str, which: Channel, id: &'static str| {
            let on = u.choices.channel == which;
            div()
                .id(SharedString::from(id))
                .px_2()
                .py_1()
                .rounded(sk.radius())
                .border(sk.box_border(1.))
                .border_color(if on { th.accent } else { th.text.alpha(0.2) })
                .when(on, |d| d.bg(th.accent.alpha(0.18)))
                .cursor_pointer()
                .text_size(sk.text_px(8.5))
                .text_color(if on { th.text } else { th.text.alpha(0.6) })
                .when(sk.unboxed(), |d| sk.choice(d, on))
                .child(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        if ws.update.choices.channel != which {
                            ws.update_set_channel(which, cx);
                        }
                    }),
                )
        };

        let installable = u.install.as_ref().is_some_and(Install::can_install);
        let mut panel = div()
            .id("update-panel")
            .w(px(520.))
            .max_h(px(self.tray_card_h()))
            .overflow_x_hidden()
            .overflow_y_scroll()
            .p_3()
            .rounded(sk.radius())
            .border_2()
            .border_color(th.accent.alpha(0.85))
            .bg(darken(th.surface, 0.6))
            .shadow(float_shadows(th.accent, th.grade.phosphor))
            .map(|d| sk.finish(d))
            .flex()
            .flex_col()
            .gap_2()
            .text_size(sk.text_px(10.))
            .text_color(th.text)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_size(sk.text_px(13.))
                    .when(!sk.text_controls(), |d| {
                        d.child(div().text_size(sk.text_px(18.)).child("\u{2b73}"))
                    })
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::EXTRA_BOLD)
                            .text_color(th.complement)
                            .child(ui.cfg_updates),
                    ),
            )
            .child(
                div()
                    .text_size(sk.text_px(8.5))
                    .text_color(th.accent.alpha(0.85))
                    .child(ui.upd_blurb),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(chip(ui.upd_stable, Channel::Stable, "update-stable"))
                    .child(chip(ui.upd_dev, Channel::Dev, "update-dev")),
            )
            .when(u.choices.channel == Channel::Dev, |d| {
                d.child(note(ui.upd_dev_band.into(), true))
            })
            .child(fact(ui.upd_running, me.describe()))
            .children(
                u.install
                    .as_ref()
                    .map(|i| fact(ui.upd_installed_as, update::describe_install(i))),
            )
            .children(
                u.record
                    .previous
                    .as_ref()
                    .map(|k| fact(ui.upd_previous, k.describe.clone())),
            )
            .child(fact(
                ui.upd_checked,
                u.record
                    .last_check
                    .map(|t| ago(update::now_unix().saturating_sub(t)))
                    .unwrap_or_else(|| ui.upd_never.into()),
            ));

        // An install TD does not update says so and why, and offers no button
        // that would pretend otherwise.
        if let Some(i) = u.install.as_ref().filter(|i| !i.can_install()) {
            panel = panel.child(note(
                ui.upd_cannot.replace("{why}", &update::refusal(i)),
                false,
            ));
        }

        panel = panel.child(
            row(
                "\u{21bb}",
                ui.upd_check_name.into(),
                ui.upd_check_blurb.into(),
                !u.busy,
                false,
                "update-check",
            )
            .when(!u.busy, |d| {
                d.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.update_check(cx);
                    }),
                )
            }),
        );
        if u.busy {
            panel = panel.child(note(ui.upd_busy.into(), false));
        }

        match &u.check {
            Some(Err(e)) => panel = panel.child(note(ui.upd_failed.replace("{why}", e), true)),
            Some(Ok(c)) => {
                if let Some(none) = &c.none {
                    panel = panel.child(note(none.clone(), false));
                }
                if let Some(o) = &c.offer {
                    let mut when = o
                        .date
                        .as_deref()
                        .map(|d| d.get(..10).unwrap_or(d).to_string())
                        .unwrap_or_default();
                    if let Some(n) = o.ahead_by.filter(|n| *n > 0) {
                        when.push_str(&format!(" · +{n}"));
                    }
                    panel =
                        panel.child(fact(ui.upd_available, format!("{} · {when}", o.describe())));
                    let notes: String = o
                        .notes
                        .lines()
                        .filter(|l| !l.trim().is_empty())
                        .take(8)
                        .collect::<Vec<_>>()
                        .join("\n");
                    if !notes.is_empty() {
                        panel = panel.child(
                            div()
                                .pl_2()
                                .border_l_2()
                                .border_color(th.text.alpha(0.2))
                                .text_size(sk.text_px(8.))
                                .text_color(th.text.alpha(0.7))
                                .child(SharedString::from(notes)),
                        );
                    }
                    if o.host_restart {
                        panel = panel.child(note(ui.upd_host_fix.into(), true));
                    }
                    let older = c.channel == Channel::Stable
                        && matches!(
                            o.relation,
                            update::Relation::Older | update::Relation::Diverged
                        );
                    if o.is_update() || older {
                        if let Some(why) = o.not_installable() {
                            panel = panel
                                .child(note(ui.upd_not_installable.replace("{why}", why), true));
                        } else if installable && u.installed.is_none() && u.prepared.is_none() {
                            let size = o
                                .size
                                .map(crate::uninstall::builds::human_bytes)
                                .unwrap_or_default();
                            if older {
                                panel = panel
                                    .child(note(
                                        ui.upd_older.replace("{version}", &o.version),
                                        false,
                                    ))
                                    .child(
                                        row(
                                            "\u{2935}",
                                            ui.upd_older_name.replace("{version}", &o.version),
                                            ui.upd_older_blurb.into(),
                                            !u.busy,
                                            true,
                                            "update-older",
                                        )
                                        .when(
                                            !u.busy,
                                            |d| {
                                                d.on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(
                                                        |ws, _: &MouseDownEvent, _w, cx| {
                                                            cx.stop_propagation();
                                                            if ws.update.armed_older {
                                                                ws.update_install(cx);
                                                            } else {
                                                                ws.update.armed_older = true;
                                                                cx.notify();
                                                            }
                                                        },
                                                    ),
                                                )
                                            },
                                        ),
                                    );
                            } else {
                                panel = panel.child(
                                    row(
                                        "\u{2913}",
                                        ui.upd_update_name.replace("{version}", &o.describe()),
                                        ui.upd_update_blurb.replace("{size}", &size),
                                        !u.busy,
                                        false,
                                        "update-install",
                                    )
                                    .when(!u.busy, |d| {
                                        d.on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                                cx.stop_propagation();
                                                ws.update_install(cx);
                                            }),
                                        )
                                    }),
                                );
                            }
                        }
                    } else {
                        panel = panel.child(note(ui.upd_uptodate.into(), false));
                    }
                }
                if let Some(lines) = &c.source_build {
                    panel = panel.child(note(ui.upd_source.into(), false)).child(
                        div()
                            .p_2()
                            .rounded(sk.radius())
                            .bg(th.text.alpha(0.06))
                            .text_size(sk.text_px(8.))
                            .child(SharedString::from(lines.clone())),
                    );
                }
            }
            None => {}
        }

        // After an install or rollback: restart, with what it costs named.
        if let Some(done) = &u.installed {
            panel = panel.child(note(
                ui.upd_backup
                    .replace("{path}", &done.backup.display().to_string()),
                false,
            ));
            if u.installed_compat == Some(Compat::Same) {
                let mut losses = vec![];
                if self.attach.is_none() {
                    losses.push(ui.upd_loss_unhosted.to_string());
                }
                if held > 0 {
                    losses.push(ui.upd_loss_held.replace("{n}", &held.to_string()));
                }
                panel = panel.child(
                    row(
                        "\u{27f3}",
                        ui.upd_restart_name.into(),
                        ui.upd_restart_blurb.replace("{n}", &panes.to_string()),
                        true,
                        false,
                        "update-restart",
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.update_restart_window(cx);
                        }),
                    ),
                );
                for l in losses {
                    panel = panel.child(note(l, true));
                }
                panel = panel.child(note(ui.upd_no_loss.into(), false)).child(note(
                    ui.upd_later.replace("{version}", &done.describe),
                    false,
                ));
            } else {
                panel = panel.child(note(
                    ui.upd_wire_warn.replace("{n}", &panes.to_string()),
                    true,
                ));
                panel = self.full_restart_row(panel, &row, panes, cx);
            }
        }

        // A staged build: installs at the next cold start unless the person
        // chooses to end the terminals now.
        let staged = u
            .prepared
            .as_ref()
            .map(|p| p.offer.describe())
            .or_else(|| u.record.pending.as_ref().map(|p| p.describe.clone()));
        if let (Some(staged), None) = (staged, &u.installed) {
            panel = panel
                .child(note(
                    ui.upd_wire_warn.replace("{n}", &panes.to_string()),
                    true,
                ))
                .child(note(ui.upd_staged.replace("{version}", &staged), false));
            panel = self.full_restart_row(panel, &row, panes, cx);
        }

        // The way back.
        if let (Some(kept), true, None) = (&u.record.previous, installable, &u.installed) {
            panel = panel.child(
                row(
                    "\u{21b6}",
                    ui.upd_rollback_name.replace("{version}", &kept.describe),
                    ui.upd_rollback_blurb.into(),
                    !u.busy,
                    false,
                    "update-rollback",
                )
                .when(!u.busy, |d| {
                    d.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.update_rollback(cx);
                        }),
                    )
                }),
            );
        }

        let daily = u.choices.check_daily == Some(true);
        panel = panel.child(
            row(
                "\u{25f7}",
                if daily {
                    ui.upd_daily_on
                } else {
                    ui.upd_daily_off
                }
                .into(),
                ui.upd_daily_blurb.into(),
                true,
                false,
                "update-daily",
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    ws.update_toggle_daily(cx);
                }),
            ),
        );
        if let Some(e) = &u.error {
            panel = panel.child(note(ui.upd_failed.replace("{why}", e), true));
        }

        Some(
            div()
                .absolute()
                .inset_0()
                .bg(th.bg.alpha(0.70))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        ws.close_updates();
                        cx.notify();
                    }),
                )
                .child(panel),
        )
    }

    fn full_restart_row<R>(
        &self,
        panel: gpui::Stateful<gpui::Div>,
        row: &R,
        panes: usize,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div>
    where
        R: Fn(&str, String, String, bool, bool, &'static str) -> gpui::Stateful<gpui::Div>,
    {
        let ui = self.lang.strings().ui;
        let others = self.update.others;
        if others > 0 {
            return panel.child(div().text_size(px(10.)).child(SharedString::from(
                ui.upd_full_blocked.replace("{n}", &others.to_string()),
            )));
        }
        let n = panes.to_string();
        let blurb = if self.update.armed_full {
            ui.upd_full_confirm.replace("{n}", &n)
        } else {
            ui.upd_full_blurb.replace("{n}", &n)
        };
        panel.child(
            row(
                "\u{23fb}",
                ui.upd_full_name.into(),
                blurb,
                true,
                true,
                "update-full",
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    ws.update_restart_everything(cx);
                }),
            ),
        )
    }
}

/// `3 min ago`, `5 h ago`, `2 d ago` — numbers and units only, which read the
/// same in every language the panel speaks.
fn ago(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs} s"),
        60..=3599 => format!("{} min", secs / 60),
        3600..=86_399 => format!("{} h", secs / 3600),
        _ => format!("{} d", secs / 86_400),
    }
}
