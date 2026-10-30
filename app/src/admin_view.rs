//! ⚙ CONFIG › ADMIN: export, old builds and uninstall, in a centred panel in
//! the style of the bug report. Nothing destructive happens on a first click:
//! pruning and removal each ask a second time, and removal only runs a plan
//! the person has opened and read. On a machine with a protection marker the
//! removal rows are locked and only the read-only preview and the agent
//! handoff are offered; the worker refuses there too, so the lock is not the
//! only thing standing between a click and the Legion.
use super::*;
use crate::uninstall::{self, builds, export, guard, plan::Retention, roots::Roots};

#[derive(Default)]
pub(crate) struct AdminState {
    export_busy: bool,
    export_done: Option<Result<String, String>>,
    /// Outer `None` while counting; inner `None` when the records cannot all be
    /// read, which is shown as unknown and never as zero.
    records_bytes: Option<Option<u64>>,
    builds: Option<Result<BuildCount, String>>,
    prune_armed: bool,
    prune_busy: bool,
    prune_done: Option<Result<String, String>>,
    /// Why removal refuses on this machine; `None` when it may run.
    protected: Option<String>,
    host: String,
    retention: Keep,
    /// The plan the person opened: the choice it was made for, its file, and
    /// the hash of the bytes they read. Uninstall runs exactly those bytes.
    reviewed: Option<(Retention, std::path::PathBuf, String)>,
    remove_armed: bool,
    remove_done: Option<Result<String, String>>,
}

/// `Retention` with the panel's default, which is to keep records.
#[derive(Clone, Copy, PartialEq)]
struct Keep(Retention);

impl Default for Keep {
    fn default() -> Self {
        Keep(Retention::Keep)
    }
}

#[derive(Clone)]
struct BuildCount {
    total: usize,
    bytes: u64,
    in_use: usize,
    free: usize,
    free_bytes: u64,
    /// Processes that may be TD and cannot be read: while there are any, which
    /// builds run is unknown, the counts above are not facts, and pruning
    /// refuses, so the panel says so instead of offering it.
    uncertain: Vec<u32>,
}

fn count_builds(roots: &Roots) -> Result<BuildCount, String> {
    count_builds_with(roots, &builds::live_procs)
}

fn count_builds_with(
    roots: &Roots,
    procs: &dyn Fn() -> Vec<(u32, std::path::PathBuf)>,
) -> Result<BuildCount, String> {
    let listing = builds::list(roots, procs);
    if let Some(e) = &listing.error {
        return Err(e.clone());
    }
    let free = builds::prunable(&listing, builds::DEFAULT_KEEP);
    Ok(BuildCount {
        total: listing.builds.len(),
        bytes: listing.builds.iter().map(|b| b.bytes).sum(),
        in_use: listing
            .builds
            .iter()
            .filter(|b| b.launcher || !b.running.is_empty())
            .count(),
        free: free.len(),
        free_bytes: free.iter().map(|b| b.bytes).sum(),
        uncertain: listing.uncertain.clone(),
    })
}

/// The old-builds row's sentence. An unknown process comes first: no count of
/// what is in use or removable is shown while the engine would refuse.
fn builds_blurb(
    ui: &crate::lang::Chrome,
    builds: &Option<Result<BuildCount, String>>,
    armed: bool,
) -> String {
    match builds {
        None => ui.admin_builds_counting.to_string(),
        Some(Err(e)) => ui.admin_failed.replace("{why}", e),
        Some(Ok(c)) if !c.uncertain.is_empty() => ui
            .admin_builds_unknown
            .replace("{n}", &c.total.to_string())
            .replace("{size}", &builds::human_bytes(c.bytes))
            .replace(
                "{pids}",
                &c.uncertain
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        Some(Ok(c)) if c.free == 0 => ui
            .admin_builds_none
            .replace("{n}", &c.total.to_string())
            .replace("{size}", &builds::human_bytes(c.bytes)),
        Some(Ok(c)) if armed => ui
            .admin_builds_confirm
            .replace("{free}", &c.free.to_string())
            .replace("{free_size}", &builds::human_bytes(c.free_bytes)),
        Some(Ok(c)) => ui
            .admin_builds_blurb
            .replace("{n}", &c.total.to_string())
            .replace("{size}", &builds::human_bytes(c.bytes))
            .replace("{used}", &c.in_use.to_string())
            .replace("{free}", &c.free.to_string())
            .replace("{free_size}", &builds::human_bytes(c.free_bytes)),
    }
}

fn can_prune(builds: &Option<Result<BuildCount, String>>) -> bool {
    matches!(builds, Some(Ok(c)) if c.free > 0 && c.uncertain.is_empty())
}

impl Workspace {
    pub(crate) fn open_admin(&mut self, cx: &mut Context<Self>) {
        self.admin_menu = true;
        let a = &mut self.admin;
        a.prune_armed = false;
        a.remove_armed = false;
        a.export_done = None;
        a.prune_done = None;
        a.remove_done = None;
        let machine = guard::Machine::read();
        a.host = machine
            .hostname
            .clone()
            .unwrap_or_else(|| "this machine".into());
        let Ok(roots) = Roots::current() else {
            a.protected = Some("HOME is not set, so nothing can be checked".into());
            return;
        };
        let p = guard::protection(&roots, &machine);
        a.protected = p.refuses().then(|| match &p {
            guard::Protection::ThisMachine { marker, .. }
            | guard::Protection::Unreadable { marker, .. } => marker.display().to_string(),
            _ => String::new(),
        });
        a.builds = None;
        cx.spawn(async move |this, cx| {
            let (count, records) = cx
                .background_executor()
                .spawn(async move {
                    // What the export would copy, exclusions already left out.
                    let records = export::plan_size(&roots, true).map(|(_, bytes)| bytes);
                    (count_builds(&roots), records)
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.admin.builds = Some(count);
                ws.admin.records_bytes = Some(records);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn close_admin(&mut self) {
        self.admin_menu = false;
        self.admin.prune_armed = false;
        self.admin.remove_armed = false;
    }

    fn admin_export(&mut self, records: bool, cx: &mut Context<Self>) {
        if self.admin.export_busy {
            return;
        }
        self.admin.export_busy = true;
        self.admin.export_done = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move {
                    let roots = Roots::current()?;
                    let opts = export::ExportOptions {
                        records,
                        dest: None,
                        stamp: uninstall::now_stamp(),
                    };
                    let host = guard::Machine::read().hostname;
                    export::export(&roots, &opts, env!("CARGO_PKG_VERSION"), host.as_deref())
                        .map(|r| r.dir.display().to_string())
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.admin.export_busy = false;
                ws.admin.export_done = Some(done);
                cx.notify();
            });
        })
        .detach();
    }

    fn admin_prune(&mut self, cx: &mut Context<Self>) {
        if self.admin.prune_busy {
            return;
        }
        if !self.admin.prune_armed {
            self.admin.prune_armed = true;
            cx.notify();
            return;
        }
        self.admin.prune_armed = false;
        self.admin.prune_busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let (done, count) = cx
                .background_executor()
                .spawn(async move {
                    let roots = match Roots::current() {
                        Ok(r) => r,
                        Err(e) => return (Err(e), None),
                    };
                    let p = builds::prune(&roots, builds::DEFAULT_KEEP, false, &builds::live_procs);
                    let freed = p.freed;
                    let done = if p.failed.is_empty() {
                        Ok((p.removed.len(), freed))
                    } else {
                        Err(p
                            .failed
                            .iter()
                            .map(|(n, e)| format!("{n}: {e}"))
                            .collect::<Vec<_>>()
                            .join("; "))
                    };
                    (done, Some(count_builds(&roots)))
                })
                .await;
            let _ = this.update(cx, |ws, cx| {
                let ui = ws.lang.strings().ui;
                ws.admin.prune_busy = false;
                ws.admin.prune_done = Some(done.map(|(n, freed)| {
                    ui.admin_builds_done
                        .replace("{n}", &n.to_string())
                        .replace("{size}", &builds::human_bytes(freed))
                }));
                if let Some(c) = count {
                    ws.admin.builds = Some(c);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Write the plan (or, on a protected machine, the same read-only preview)
    /// and open it where the person can read every line.
    fn admin_review(&mut self, cx: &mut Context<Self>) {
        let retention = self.admin.retention.0;
        let written = Roots::current().and_then(|roots| {
            let plan = uninstall::make_plan(&roots, retention, &uninstall::now_stamp());
            uninstall::write_review(&roots, &plan)
        });
        match written {
            Ok(r) => {
                self.admin.reviewed = Some((retention, r.json, r.sha256));
                self.admin.remove_armed = false;
                if self.doc_here(&r.md, cx).starts_with("err") {
                    pane::open_with_system(&r.md.to_string_lossy());
                }
            }
            Err(e) => self.admin.remove_done = Some(Err(e)),
        }
    }

    fn admin_handoff(&mut self, cx: &mut Context<Self>) {
        let retention = self.admin.retention.0;
        let written = Roots::current().and_then(|roots| {
            let plan = uninstall::make_plan(&roots, retention, &uninstall::now_stamp());
            uninstall::write_handoff(&roots, &plan)
        });
        match written {
            Ok(path) => {
                if self.doc_here(&path, cx).starts_with("err") {
                    pane::open_with_system(&path.to_string_lossy());
                }
            }
            Err(e) => self.admin.remove_done = Some(Err(e)),
        }
    }

    fn admin_remove(&mut self, cx: &mut Context<Self>) {
        let retention = self.admin.retention.0;
        let Some((reviewed_for, json, sha)) = self.admin.reviewed.clone() else {
            return;
        };
        if self.admin.protected.is_some() || reviewed_for != retention {
            return;
        }
        if !self.admin.remove_armed {
            self.admin.remove_armed = true;
            cx.notify();
            return;
        }
        self.admin.remove_armed = false;
        let started = Roots::current().and_then(|roots| {
            let plan: uninstall::plan::Plan = std::fs::read_to_string(&json)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))?;
            if plan.retention != reviewed_for {
                return Err("the reviewed plan is for a different choice; review it again".into());
            }
            let out = uninstall::removal_dir(&roots, &plan.stamp);
            uninstall::spawn_worker(&json, Some(&sha), &out)?;
            Ok(out.join("receipt.md").display().to_string())
        });
        self.admin.remove_done = Some(started);
        cx.notify();
    }

    /// ⚙ the ADMIN panel.
    pub(crate) fn render_admin_overlay(
        &self,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        if !self.admin_menu {
            return None;
        }
        let ui = self.lang.strings().ui;
        let sk = skin::skin(cx, th.grade.scale);
        let a = &self.admin;

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
                    .border_1()
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
                    .child(
                        div()
                            .text_size(sk.text_px(15.))
                            .child(SharedString::from(glyph.to_string())),
                    )
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
        let section = |text: &'static str| {
            div()
                .pt_1()
                .text_size(sk.text_px(8.))
                .font_weight(gpui::FontWeight::EXTRA_BOLD)
                .text_color(th.accent.alpha(0.85))
                .child(text)
        };
        let done_note = |d: &Option<Result<String, String>>, ok_fmt: &str| {
            d.as_ref().map(|r| match r {
                Ok(t) => note(ok_fmt.replace("{path}", t), false),
                Err(e) => note(ui.admin_failed.replace("{why}", e), true),
            })
        };

        let records = match a.records_bytes {
            None => "…".to_string(),
            Some(Some(bytes)) => builds::human_bytes(bytes),
            Some(None) => "?".to_string(),
        };
        let builds_blurb = builds_blurb(ui, &a.builds, a.prune_armed);
        let can_prune = can_prune(&a.builds) && !a.prune_busy;

        let mut panel = div()
            .id("admin-panel")
            .w(px(500.))
            .max_h(px(self.tray_card_h()))
            .overflow_x_hidden()
            .overflow_y_scroll()
            .p_3()
            .rounded(sk.radius())
            .border_2()
            .border_color(th.accent.alpha(0.85))
            .bg(darken(th.surface, 0.6))
            .shadow(float_shadows(th.accent, th.grade.phosphor))
            // Finished before any content, as the bug panel learned: a cut
            // skin's plate added after the rows paints over them.
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
                    .child(div().text_size(sk.text_px(18.)).child("\u{2699}"))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::EXTRA_BOLD)
                            .text_color(th.complement)
                            .child(ui.admin_hdr),
                    ),
            )
            .child(
                div()
                    .text_size(sk.text_px(8.5))
                    .text_color(th.accent.alpha(0.85))
                    .child(ui.admin_blurb),
            )
            .child(section(ui.admin_export_section))
            .child(
                row(
                    "\u{1f4e6}",
                    ui.admin_export_name.into(),
                    ui.admin_export_blurb.into(),
                    !a.export_busy,
                    false,
                    "admin-export",
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.admin_export(false, cx);
                    }),
                ),
            )
            .child(
                row(
                    "\u{1f5c3}",
                    ui.admin_export_all_name.into(),
                    ui.admin_export_all_blurb.replace("{size}", &records),
                    !a.export_busy,
                    false,
                    "admin-export-all",
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.admin_export(true, cx);
                    }),
                ),
            )
            .when(a.export_busy, |d| {
                d.child(note(ui.admin_exporting.into(), false))
            })
            .children(done_note(&a.export_done, ui.admin_exported))
            .child(section(ui.admin_builds_section))
            .child(
                row(
                    "\u{1f9f9}",
                    ui.admin_builds_name.into(),
                    builds_blurb,
                    can_prune,
                    a.prune_armed,
                    "admin-prune",
                )
                .when(can_prune, |d| {
                    d.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.admin_prune(cx);
                        }),
                    )
                }),
            )
            .when(a.prune_busy, |d| {
                d.child(note(ui.admin_pruning.into(), false))
            })
            .children(done_note(&a.prune_done, "{path}"))
            .child(section(ui.admin_uninstall_section));

        if let Some(marker) = &a.protected {
            panel = panel
                .child(row(
                    "\u{1f512}",
                    ui.admin_remove_name.into(),
                    ui.admin_protected
                        .replace("{host}", &a.host)
                        .replace("{marker}", marker),
                    false,
                    true,
                    "admin-locked",
                ))
                .child(
                    row(
                        "\u{1f4c4}",
                        ui.admin_preview_name.into(),
                        ui.admin_preview_blurb.into(),
                        true,
                        false,
                        "admin-preview",
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.admin_review(cx);
                            cx.notify();
                        }),
                    ),
                );
        } else {
            let chip = |label: &'static str, which: Retention, id: &'static str| {
                let on = a.retention.0 == which;
                div()
                    .id(SharedString::from(id))
                    .px_2()
                    .py_1()
                    .rounded(sk.radius())
                    .border_1()
                    .border_color(if on { th.accent } else { th.text.alpha(0.2) })
                    .when(on, |d| d.bg(th.accent.alpha(0.18)))
                    .cursor_pointer()
                    .text_size(sk.text_px(8.5))
                    .text_color(if on { th.text } else { th.text.alpha(0.6) })
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.admin.retention = Keep(which);
                            ws.admin.remove_armed = false;
                            cx.notify();
                        }),
                    )
            };
            let reviewed = a
                .reviewed
                .as_ref()
                .is_some_and(|(r, _, _)| *r == a.retention.0);
            let remove_blurb = if !reviewed {
                ui.admin_remove_first.to_string()
            } else if a.remove_armed {
                ui.admin_remove_confirm.to_string()
            } else {
                ui.admin_remove_blurb.to_string()
            };
            panel = panel
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap_1()
                        .child(chip(ui.admin_keep, Retention::Keep, "admin-keep"))
                        .child(chip(
                            ui.admin_export_then,
                            Retention::Export,
                            "admin-export-then",
                        ))
                        .child(chip(ui.admin_delete, Retention::Delete, "admin-delete")),
                )
                .child(
                    row(
                        "\u{1f4c4}",
                        ui.admin_review_name.into(),
                        ui.admin_review_blurb.into(),
                        true,
                        false,
                        "admin-review",
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.admin_review(cx);
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    row(
                        "\u{1f5d1}",
                        ui.admin_remove_name.into(),
                        remove_blurb,
                        reviewed,
                        a.remove_armed,
                        "admin-remove",
                    )
                    .when(reviewed, |d| {
                        d.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.admin_remove(cx);
                            }),
                        )
                    }),
                )
                .children(done_note(&a.remove_done, ui.admin_remove_started));
        }
        panel = panel.child(
            row(
                "\u{1f916}",
                ui.admin_handoff_name.into(),
                ui.admin_handoff_blurb.into(),
                true,
                false,
                "admin-handoff",
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    ws.admin_handoff(cx);
                    cx.notify();
                }),
            ),
        );

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
                        ws.close_admin();
                        cx.notify();
                    }),
                )
                .child(panel),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_whose_use_cannot_be_read_offers_no_count_and_no_prune() {
        let home = std::env::temp_dir().join(format!("td-admin-unknown-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let lib = home.join(".local/lib/terminal-delight");
        std::fs::create_dir_all(&lib).unwrap();
        for n in ["td-a-1", "td-b-2", "td-c-3", "td-d-4"] {
            std::fs::write(lib.join(n), b"\x7fELF\x02\x01\x01\x00\x00\x00\x00").unwrap();
        }
        let h = home.clone();
        let roots =
            Roots::from_env(&move |k| (k == "HOME").then(|| std::ffi::OsString::from(&h))).unwrap();
        // A process that may be TD and whose executable cannot be read.
        let procs = || {
            vec![
                (std::process::id(), std::env::current_exe().unwrap()),
                (4711, std::path::PathBuf::new()),
            ]
        };
        let count = count_builds_with(&roots, &procs).unwrap();
        assert_eq!(count.uncertain, vec![4711]);
        let builds = Some(Ok(count));
        let ui = crate::lang::Lang::En.strings().ui;
        for armed in [false, true] {
            let text = builds_blurb(ui, &builds, armed);
            assert!(text.contains("4711") && text.contains("unknown"), "{text}");
            assert!(!text.contains("nothing runs"), "{text}");
        }
        assert!(!can_prune(&builds));
        std::fs::remove_dir_all(home).unwrap();
    }
}
