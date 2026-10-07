//! What this pane's agent handed over, as the terminal view keeps it, files it
//! and shows it.
//!
//! A child module of `pane` for the reason `pane/bench.rs` gives: it reaches
//! the pane's private fields (`self.bench`, `self.wb_conv`, `self.float`)
//! without any of them going `pub(crate)`. The rules — which screen rows are a
//! handover, what one handover is, what the page says — live in
//! [`crate::handover`] as pure functions with table tests. This is the wiring.
//!
//! ```text
//!   the verb, the screen, the bench  →  hand_over / handover_presented
//!                                         │
//!                     ┌───────────────────┼─────────────────────┐
//!                     ▼                   ▼                     ▼
//!          the conversation's record   the ledger         the history page
//!          (bench_file_docs)           (self.handovers)   (handed-over.md)
//!                                         │                     │
//!                                         ▼                     ▼
//!                                  the header chip  ──▾──→  the floating square
//! ```

use super::*;
use crate::handover::{self, Handover, Recorded, Source};

impl TerminalView {
    /// Take one handover from the verb or the screen: keep it, file it into
    /// the conversation's record so it outlives the window, and keep the
    /// history page current.
    ///
    /// The same link handed over again changes nothing. A line read off the
    /// screen that the agent then declares is the same row with the stronger
    /// word, refiled so the record says `declared` too.
    pub(crate) fn hand_over(
        &mut self,
        label: Option<String>,
        href: String,
        source: Source,
        cx: &mut Context<Self>,
    ) -> Recorded {
        let label = label
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| handover::fallback_label(&href));
        let incoming = Handover {
            label,
            href,
            at_ms: crate::surfacefeed::now_ms(),
            source,
        };
        let key = incoming.key();
        let done = self.handovers.record(incoming);
        if done == Recorded::Known {
            return done;
        }
        let Some(kept) = self.handovers.get(&key).cloned() else {
            return done;
        };
        // An artifact the agent put on its own bench for this same file is
        // already the bench's card and already in the record. Filing a second
        // copy would put two cards for one file on its shelf.
        let theirs = self
            .bench
            .artifacts()
            .any(|(id, href)| !id.0.starts_with("handover-") && handover::key_of(href) == key);
        if !theirs {
            let doc = handover::surface_doc(&kept);
            let id = handover::surface_id(&key);
            // Filed BEFORE it is drawn, the order `deliver_surfaces` keeps.
            self.bench_file_docs(&[(id.clone(), doc.clone())]);
            self.present(crate::surface::parse_lenient(&doc, kept.at_ms, &id), cx);
        }
        self.refresh_handover_page();
        cx.notify();
        done
    }

    /// An artifact arrived on the bench by the agent's own hand — the MCP
    /// verb, a dropped file, a replay of the record. It is a handover too, and
    /// it is already filed, so it is only kept.
    pub(crate) fn handover_presented(&mut self, surface: &crate::surface::Surface) {
        if surface.id.0.starts_with("handover-") {
            return;
        }
        let crate::surface::Kind::Artifact(a) = &surface.kind else {
            return;
        };
        let label = if surface.title.trim().is_empty() {
            handover::fallback_label(&a.href)
        } else {
            surface.title.clone()
        };
        let got = self.handovers.record(Handover {
            label,
            href: a.href.clone(),
            at_ms: surface.arrived_ms,
            source: Source::Presented,
        });
        if got != Recorded::Known {
            self.refresh_handover_page();
        }
    }

    /// Rebuild the list from the conversation's whole record, and its opened
    /// marks from beside it. Called when the pane is bound to a conversation:
    /// a window restart, a resume, or the first sweep that can name it.
    ///
    /// The whole record, not the bench's 64: a long-running agent's oldest
    /// handovers have left the bench long before anybody goes looking for
    /// them, and that is exactly when somebody does.
    pub(crate) fn handovers_from_record(&mut self, root: &str) {
        let dir = crate::benchstore::store_root();
        let mut ledger = handover::Ledger::default();
        for rec in crate::benchstore::records(&dir, root) {
            if let crate::benchstore::Rec::Said { at_ms, surface, .. } = rec {
                if let Some(h) = handover::from_surface(&surface, at_ms) {
                    ledger.record(h);
                }
            }
        }
        if let Some(path) = handover::opened_path(root) {
            ledger.restore_opened(handover::read_opened(&path));
        }
        self.handovers = ledger;
        self.refresh_handover_page();
    }

    /// Read the screen for `Deliverable:` lines. Once a second, from the slow
    /// clock that already walks every agent pane: a line drawn and left for a
    /// second is caught, and a reply that ends a turn leaves its last lines on
    /// the screen until the next one.
    ///
    /// A new one also becomes this pane's needs-me link when the agent did not
    /// declare one, so the attention queue's `o` reaches the third of agents
    /// that only print the line.
    pub(crate) fn scan_handovers(&mut self, cx: &mut Context<Self>) {
        if !self.mode.is_agent() {
            return;
        }
        let rows = self.live_rows();
        if !rows.iter().any(|r| r.contains("Deliverable:")) {
            return;
        }
        for (label, href) in handover::handover_rows(&rows) {
            if self.hand_over(Some(label.clone()), href.clone(), Source::Said, cx) == Recorded::New
            {
                let declared_this = self
                    .deliverable
                    .as_ref()
                    .is_some_and(|d| handover::key_of(&d.href) == handover::key_of(&href));
                if !declared_this {
                    self.declare_deliverable(Some(crate::attention::Deliverable { label, href }));
                }
            }
        }
    }

    /// Where this pane's history page is written: its conversation's folder,
    /// or the pane's own until it has one.
    fn handover_page_path(&self) -> Option<std::path::PathBuf> {
        let root = self.wb_conv.as_ref().map(|k| k.root.as_str());
        let session = crate::surfacefeed::session().unwrap_or("unknown");
        handover::history_path(root, session, self.pane_id.unwrap_or(0))
    }

    /// Keep the history page current as things change — for a pane bound to a
    /// conversation, whose page is the conversation's and outlives the window.
    /// A pane not yet bound writes its page only when somebody opens it, so a
    /// pane that is bound a second later leaves no page of its own behind.
    fn refresh_handover_page(&self) {
        if self.wb_conv.is_some() {
            self.write_handover_page();
        }
    }

    /// Write the history page if it changed. Answers where it is, written or
    /// not, so a caller about to open it has the path.
    pub(crate) fn write_handover_page(&self) -> Option<std::path::PathBuf> {
        let path = self.handover_page_path()?;
        let now = crate::surfacefeed::now_ms();
        let date = |ms: u64| handover::local(ms).map(|c| (c.year, c.month, c.day));
        let rows: Vec<handover::Row> = self
            .handovers
            .newest_first()
            .into_iter()
            .map(|h| handover::Row {
                label: h.label.clone(),
                href: h.href.clone(),
                kind: h.kind(),
                source: h.source,
                opened: self.handovers.is_opened(&h.key()),
                when: handover::local(h.at_ms),
                fact: handover::file_fact(h, h.path().map(|p| modified_ms(&p)).unwrap_or(None)),
            })
            .collect();
        let page = handover::history_markdown(
            &self.handover_pane_name(),
            &rows,
            date(now),
            date(now.saturating_sub(86_400_000)),
        );
        if let Err(e) = handover::write_if_changed(&path, &page) {
            eprintln!("terminal-delight: {}: {e}", path.display());
            return None;
        }
        Some(path)
    }

    /// The name the page is headed with: the pane's own name, as its header
    /// shows it, without the agent's activity glyph in front.
    fn handover_pane_name(&self) -> String {
        let name = self.name.clone().unwrap_or_else(|| self.title.clone());
        name.trim_start_matches(|c: char| !c.is_alphanumeric())
            .trim()
            .to_string()
    }

    /// The chip's name, clicked: the newest handover, over this pane.
    pub(crate) fn open_newest_handover(&mut self, cx: &mut Context<Self>) {
        if let Some(h) = self.handovers.newest().cloned() {
            self.open_handover(&h, cx);
        }
    }

    /// Open one handover the way the history page's links open: a file this
    /// window can draw floats over the pane, anything else goes to the
    /// desktop, and a file that is gone says so instead of failing out of
    /// sight.
    fn open_handover(&mut self, h: &Handover, cx: &mut Context<Self>) {
        match h.path() {
            Some(path) if !path.exists() => {
                self.say(format!("{} · file gone", h.label), None, cx);
                return;
            }
            Some(path) => match crate::docopen::drawable_document(&path) {
                Some(target) => {
                    let _ = self.open_float(target, None, cx);
                }
                None => open_with_system(&path.to_string_lossy()),
            },
            None => open_with_system(&h.href),
        }
        self.mark_handover_opened(&h.key());
        cx.notify();
    }

    /// `▾` on the chip, or Ctrl+Shift+M: the history page over this pane, or
    /// away again if it is the page already showing.
    pub(crate) fn toggle_handover_page(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.write_handover_page() else {
            return;
        };
        if self.float_path(cx).is_some_and(|p| p == path) {
            self.request_close_float(cx);
            return;
        }
        if let Some(target) = crate::docopen::drawable_document(&path) {
            let _ = self.open_float(target, None, cx);
        }
        cx.notify();
    }

    /// A handover has been opened: say so on the chip and the page, and keep
    /// the mark beside the conversation's record.
    pub(crate) fn mark_handover_opened(&mut self, key: &str) {
        let now = crate::surfacefeed::now_ms();
        if !self.handovers.mark_opened(key, now) {
            return;
        }
        if let Some(path) = self
            .wb_conv
            .as_ref()
            .and_then(|k| handover::opened_path(&k.root))
        {
            if let Err(e) = handover::append_opened(&path, key, now) {
                eprintln!("terminal-delight: {}: {e}", path.display());
            }
        }
        self.refresh_handover_page();
    }

    /// A document was opened in this pane by any road — the chip, the page, an
    /// Alt+click on its path, the needs-me queue. If it is one this agent
    /// handed over, it has now been opened.
    pub(crate) fn note_opened_path(&mut self, path: &std::path::Path) {
        let key = path.to_string_lossy();
        if self.handovers.contains(&key) {
            self.mark_handover_opened(&key);
        }
    }

    /// Whether the chip has anything to name.
    pub(crate) fn has_handovers(&self) -> bool {
        !self.handovers.is_empty()
    }

    /// The header chip. `None` when this pane's agent has handed nothing over:
    /// an empty chip and a chip still being read look the same, and only one
    /// of them is true.
    ///
    /// Wide, it carries the newest handover's NAME — never a count, which is
    /// the header's own rule, kept since a lone `2` beside BENCH was taken off
    /// for nobody being able to say what it counted. Narrow, it carries the
    /// word for what the list is. Both halves are buttons: the name opens that
    /// handover over the pane, the `▾` opens the list.
    pub(super) fn handover_chip(
        &self,
        sk: &crate::skin::Skin,
        hicon: f32,
        wide: bool,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let newest = self.handovers.newest()?;
        let fresh = !self.handovers.is_opened(&newest.key());
        let text = px((hicon * 0.5).max(10.5));
        let name: String = if wide {
            ellipsize(&newest.label, CHIP_NAME_CHARS)
        } else {
            format!("▤ {}", crate::lang::current().strings().artifacts_word)
        };
        let (edge, face, ink) = if fresh {
            (sk.ink.mark_dim, sk.ink.mark_wash, sk.ink.ink_lit)
        } else {
            (sk.ink.edge_rest, sk.ink.face_rest, sk.ink.ink_dim)
        };
        let half = || {
            div()
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                .px(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(sk.ink.hover))
        };
        let label = half()
            .gap(px(5.))
            .when(fresh && wide, |d| {
                d.child(
                    div()
                        .text_size(px(7.))
                        .text_color(sk.ink.mark)
                        .child("\u{25cf}"),
                )
            })
            .child(div().text_color(ink).child(name))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _ev: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    if wide {
                        view.open_newest_handover(cx);
                    } else {
                        view.toggle_handover_page(cx);
                    }
                }),
            );
        let caret = half()
            .border_l_1()
            .border_color(edge)
            .text_color(sk.ink.ink_dim)
            .child("\u{25be}")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _ev: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    view.toggle_handover_page(cx);
                }),
            );
        Some(
            div()
                .id("handover-chip")
                .flex()
                .flex_row()
                .items_center()
                .h(px(hicon * 1.6))
                .rounded(sk.radius())
                .border_1()
                .border_color(edge)
                .bg(face)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(text)
                .child(label)
                .child(caret)
                .into_any_element(),
        )
    }
}

/// How many characters of a name the wide chip shows. Checked against the
/// sixteen handovers in the 2026-10-06 Cinema Delight pane: 26 keeps "The
/// symphony, curved glass" whole and leaves every one of them distinct.
const CHIP_NAME_CHARS: usize = 26;

/// Cut a name to `max` characters with an ellipsis, on a character boundary.
fn ellipsize(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{}\u{2026}", cut.trim_end())
}

/// A file's modification time in Unix milliseconds: `Some(None)` when the file
/// is not there, `None` when it is there and could not be read.
fn modified_ms(path: &std::path::Path) -> Option<Option<u64>> {
    match std::fs::metadata(path) {
        Ok(m) => m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| Some(d.as_millis() as u64)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(None),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_name_is_cut_with_an_ellipsis_and_a_short_one_is_left_alone() {
        assert_eq!(ellipsize("The File Drop", 26), "The File Drop");
        let cut = ellipsize("The symphony, curved glass (review cut)", 26);
        assert_eq!(cut.chars().count(), 26);
        assert!(cut.ends_with('\u{2026}'));
        assert!(cut.starts_with("The symphony, curved glass"));
        // Not split inside a character.
        assert_eq!(ellipsize("ééééé", 3).chars().count(), 3);
    }

    #[test]
    fn a_missing_file_is_gone_and_a_present_one_has_a_time() {
        let dir = std::env::temp_dir().join(format!("td-handed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x.html");
        std::fs::write(&f, "x").unwrap();
        assert!(matches!(modified_ms(&f), Some(Some(_))));
        assert_eq!(modified_ms(&dir.join("nope.html")), Some(None));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
