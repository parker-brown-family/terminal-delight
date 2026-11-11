//! The INBOX face of the left spine, drawn.
//!
//! The rules live in `inbox.rs` and are tested there without a window. This
//! file does the three things only a window can: read the live panes into
//! [`inbox::Fact`]s once a frame — beside the rail's own projection, which it
//! reads rather than repeats — draw the shelves inside the left spine, and
//! carry a row's verbs: go there, snooze, done, wake, reopen.
//!
//! The spine keeps everything else it had. The usage slot and the bay stay at
//! its foot, the resize handle stays on its edge, and PROJECTS is one press
//! away in the same header, so the inbox is a second way of reading the same
//! window and never a different window.

use super::*;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::SystemTime;

/// One frame's inbox: every shelf, and the pane each row goes to.
///
/// The rail's memo, again, for the rail's reason: the toggle's count, the
/// list and a click resolved between frames must all read one partition, and
/// building it is a walk of every pane.
pub(crate) struct InboxMemo {
    frame: u64,
    shelves: Rc<inbox::Partition>,
    panes: Rc<HashMap<u64, EntityId>>,
}

/// Where each INBOX row was drawn, by the pane it stands for: the hit boxes a
/// drag reads, rebuilt with the list every frame.
pub(crate) type InboxBoxes = Arc<Mutex<Vec<(EntityId, Bounds<Pixels>)>>>;

/// An INBOX row carried toward another agent's row. Let go over one and the
/// two are introduced through the relate dialog (`relate_view.rs`), the same
/// door a tab dropped on a tab comes through.
pub(crate) struct InboxDrag {
    /// The pane the grabbed row stands for.
    pub(crate) from: EntityId,
    pub(crate) start: Point<Pixels>,
    pub(crate) at: Point<Pixels>,
    /// Past the threshold: until then the press is a click on the row.
    pub(crate) engaged: bool,
}

/// Where the inbox's agent badges key their animations: past the strip's and
/// the tree's ranges, so a pane showing in three places animates three times
/// rather than sharing one clock between them.
const INBOX_BADGE_KEYS: usize = 1 << 21;

/// Claude's status glyphs at the front of a terminal title — `✳`, the
/// spinner's quarter moons — are the agent saying it is busy, which the row
/// already says in its own mark. Left on the title they push every name one
/// glyph to the right and tell the reader nothing the mark does not.
fn bare_title(t: &str) -> &str {
    let t = t
        .trim_start_matches(|c: char| "✳✻✽✶✢·◐◑◒◓⠂⠁⠈⠐⠠⢀⡀⠄".contains(c))
        .trim_start();
    // A shell's own title — `me@host:~/dir` — is the shell saying who and
    // where, not what the agent in it is doing. It names nothing the place
    // line does not, so the row falls back to the tab's name.
    if shell_title(t) {
        ""
    } else {
        t
    }
}

/// `user@host:path`, the title bash and zsh set by default: one word with an
/// `@` before its first `:`, and nothing after the path.
fn shell_title(t: &str) -> bool {
    let Some((who, rest)) = t.split_once(':') else {
        return false;
    };
    who.contains('@')
        && !who.contains(char::is_whitespace)
        && (rest.starts_with('~') || rest.starts_with('/'))
        && !rest.trim().contains(' ')
}

/// One line of an agent's words, for a row with room for one: the first line,
/// its whitespace squeezed, cut at a word near the end of the budget.
fn one_line(text: &str, max_chars: usize) -> String {
    let first = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let flat = first.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max_chars {
        return flat;
    }
    let cut: String = flat.chars().take(max_chars).collect();
    let at = cut
        .rfind(' ')
        .filter(|&i| i > max_chars / 2)
        .unwrap_or(cut.len());
    format!("{}\u{2026}", cut[..at].trim_end())
}

/// `TD_INBOX_DEMO=1`: see [`Workspace::inbox_demo_facts`].
fn inbox_demo() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("TD_INBOX_DEMO").is_ok_and(|v| v != "0"))
}

fn ms(t: SystemTime) -> u64 {
    t.duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

impl Workspace {
    /// Whether the spine is showing the inbox rather than the project tree.
    pub(crate) fn spine_inbox(&self) -> bool {
        self.inbox_store.borrow().showing.unwrap_or(false)
    }

    /// Show the inbox (`true`) or the project tree, and remember which.
    pub(crate) fn set_spine_inbox(&mut self, inbox: bool, cx: &mut Context<Self>) {
        self.inbox_store.borrow_mut().set_showing(inbox);
        self.inbox_snoozing = None;
        // The inbox lives in the spine, so asking for it opens the spine.
        if inbox && !self.left_bar {
            self.set_left_bar(true, cx);
            self.save(cx);
        }
        self.inbox_save();
        cx.notify();
    }

    /// The inbox for this frame, built once however many surfaces ask.
    pub(crate) fn inbox_rows(
        &self,
        cx: &App,
    ) -> (Rc<inbox::Partition>, Rc<HashMap<u64, EntityId>>) {
        if let Some(memo) = self.inbox_memo.borrow().as_ref() {
            if memo.frame == self.rail_frame {
                return (memo.shelves.clone(), memo.panes.clone());
            }
        }
        let (facts, panes) = self.inbox_facts(cx);
        let shelves = Rc::new(inbox::partition(
            &facts,
            &mut self.inbox_store.borrow_mut(),
            &mut self.inbox_tracker.borrow_mut(),
            SystemTime::now(),
            Instant::now(),
        ));
        let panes = Rc::new(panes);
        *self.inbox_memo.borrow_mut() = Some(InboxMemo {
            frame: self.rail_frame,
            shelves: shelves.clone(),
            panes: panes.clone(),
        });
        (shelves, panes)
    }

    /// What the inbox knows about every pane, read off the live panes.
    ///
    /// The lane, its age and its level come from the rail's projection for the
    /// frame, never from the pane's flags a second time — the badge decided the
    /// lane once, and the rail already asked it.
    fn inbox_facts(&self, cx: &App) -> (Vec<inbox::Fact>, HashMap<u64, EntityId>) {
        if inbox_demo() {
            return self.inbox_demo_facts();
        }
        let (items, _) = self.rail_rows(cx);
        let ranked: HashMap<u64, &attention::AttentionItem> =
            items.iter().map(|item| (item.pane, item)).collect();
        let mut facts = Vec::new();
        let mut panes = HashMap::new();
        for (ti, tab) in self.tabs.iter().enumerate() {
            let mut leaves = Vec::new();
            tab.root.leaves(&mut leaves);
            // Per tab, hoisted out of the per-pane loop: the rail learned that a
            // tab of four terminals walked the tree four times.
            let spine = self.spine_path(ti);
            let level = self.level_of(ti).priority;
            for leaf in leaves {
                let key = leaf.entity_id().as_u64();
                let v = leaf.read(cx);
                let life = if v.rest.dormancy.asleep().is_some() {
                    inbox::Life::Asleep
                } else if v.rest.dormancy.is_resting() {
                    inbox::Life::Resting
                } else {
                    match v.mode {
                        pane::PaneMode::Unknown => inbox::Life::Unknown,
                        ref mode if mode.is_agent() => inbox::Life::Awake,
                        _ => inbox::Life::Shell,
                    }
                };
                let item = ranked.get(&key);
                let name = threshing_floor::PaneName::new(&spine, &v.header_title());
                let header = bare_title(&name.header);
                let (title, place) = if header.is_empty() {
                    (name.tab.clone(), name.outer.clone())
                } else {
                    (header.to_string(), name.place())
                };
                // The agent's own latest words first — the brief it wrote for a
                // person — then the line behind its lane, then the note somebody
                // stuck on the pane. Absent is absent: no row says "no summary".
                let line = v
                    .bench
                    .latest_response()
                    .map(|r| r.brief.clone().unwrap_or_else(|| r.layman.clone()))
                    .or_else(|| v.attention_evidence())
                    .or_else(|| v.saved_note().map(|n| n.title.unwrap_or(n.text)))
                    .map(|l| one_line(&l, 160))
                    .filter(|l| !l.is_empty());
                facts.push(inbox::Fact {
                    key: v
                        .pane_id()
                        .map(inbox::Key::Pane)
                        .unwrap_or(inbox::Key::Window(key)),
                    pane: key,
                    life,
                    lane: item.map(|i| i.kind),
                    working: v.agent_working(),
                    priority: item.map(|i| i.priority).unwrap_or(level),
                    changed: item.and_then(|i| i.observed_at),
                    evidence: item.and_then(|i| i.evidence.clone()),
                    title: if title.is_empty() {
                        v.mode.label().to_string()
                    } else {
                        title
                    },
                    place,
                    lead: (life != inbox::Life::Shell).then(|| v.header_lead().into_owned()),
                    line,
                });
                panes.insert(key, leaf.entity_id());
            }
        }
        (facts, panes)
    }

    /// Once a frame, from `render`: the pane in front of the person counts as
    /// visited, what changed is written down, and a wake is booked for the
    /// next moment the shelves will move on their own — a snooze running out,
    /// or a WORKING seat's grace ending — because nothing else may draw a
    /// frame then.
    pub(crate) fn inbox_tick(&mut self, looking: bool, cx: &mut Context<Self>) {
        // Visited only when the person can actually be looking at it: the
        // window active and no page or panel over the panes. A WOKE row behind
        // the composer, or in a window on another workspace, has not been seen.
        let focused = self
            .tabs
            .get(self.active)
            .and_then(|t| t.focused)
            .filter(|_| looking);
        if let Some(focused) = focused {
            if let Some(view) = self.pane_by_id(focused) {
                let key = view
                    .read(cx)
                    .pane_id()
                    .map(inbox::Key::Pane)
                    .unwrap_or(inbox::Key::Window(focused.as_u64()));
                self.inbox_store.borrow_mut().visited(key);
            }
        }
        let (shelves, _) = self.inbox_rows(cx);
        self.inbox_save();
        let next_wake = self
            .inbox_store
            .borrow()
            .snoozed
            .values()
            .map(|s| s.until)
            .min();
        let holding = shelves.working.iter().any(|r| !r.fact.working);
        // The earliest moment anything moves: a snooze's hour, or — while a
        // stopped agent holds its WORKING seat — the end of the grace, which is
        // at most WORKING_GRACE away.
        let grace = holding.then(|| SystemTime::now() + inbox::WORKING_GRACE);
        let due = match (next_wake, grace) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        // Book only what is EARLIER than what is already booked. Booking every
        // frame would spawn a timer a frame while a seat is held, and the one
        // already booked will wake the window in time to book the next.
        let Some(due) = due else {
            return;
        };
        if self.inbox_clock.is_some_and(|booked| booked <= due) {
            return;
        }
        self.inbox_clock = Some(due);
        self.inbox_clock_epoch = self.inbox_clock_epoch.wrapping_add(1);
        let epoch = self.inbox_clock_epoch;
        let wait = due
            .duration_since(SystemTime::now())
            .unwrap_or_default()
            .saturating_add(Duration::from_millis(250));
        cx.spawn(async move |ws, cx| {
            cx.background_executor().timer(wait).await;
            let _ = ws.update(cx, |ws, cx| {
                if ws.inbox_clock_epoch == epoch {
                    ws.inbox_clock = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Write the inbox down if anything in it changed. The file is small and
    /// written whole, through the same atomic rename the session file uses.
    pub(crate) fn inbox_save(&self) {
        if !self.inbox_store.borrow_mut().take_dirty() || inbox_demo() {
            return;
        }
        // The session file's own guard: a scratch window, one whose host is
        // gone, or one that has handed its session on writes nothing.
        if self.save_blocked().is_some() {
            return;
        }
        let text = inbox::encode(&self.inbox_store.borrow());
        let path = inbox::path_for(&instance::config_dir(), instance::key());
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = session::write_atomic(&path, &text) {
            eprintln!("terminal-delight: inbox not saved: {e}");
        }
    }

    /// A fixed, fictional fleet for looking at the inbox: `TD_INBOX_DEMO=1`.
    ///
    /// The rail has `TD_SPINE_DEMO` for the reason this exists: the shelves
    /// only fill when real agents are in every state at once — one asking, one
    /// failed, one asleep, one snoozed until morning — and no rig can stage
    /// that on demand. Every name, line and model here is invented, the house
    /// rule for demo data; rows go to the window's real panes round-robin, so
    /// a click still lands somewhere real. Keys are window-only, so nothing
    /// about the demo is ever written down.
    fn inbox_demo_facts(&self) -> (Vec<inbox::Fact>, HashMap<u64, EntityId>) {
        use attention::AttentionKind as K;
        let mut live: Vec<EntityId> = Vec::new();
        for tab in &self.tabs {
            let mut leaves = Vec::new();
            tab.root.leaves(&mut leaves);
            live.extend(leaves.iter().map(|l| l.entity_id()));
        }
        let ago = |secs: u64| Instant::now().checked_sub(Duration::from_secs(secs));
        #[allow(clippy::type_complexity)]
        let rows: [(
            &str,
            &str,
            &str,
            Option<K>,
            bool,
            inbox::Life,
            Option<u64>,
            Option<&str>,
        ); 18] = [
            (
                "Rotate the staging keys",
                "ATLAS \u{203a} ingest",
                "OPUS 5.5 MAX",
                Some(K::Decision),
                false,
                inbox::Life::Awake,
                Some(41 * 60),
                Some("Do you want to proceed? \u{276f} 1. Rotate now  2. Stage first"),
            ),
            (
                "Reconcile October",
                "LEDGER \u{203a} invoices",
                "SONNET 5.5 HIGH",
                Some(K::Access),
                false,
                inbox::Life::Awake,
                Some(20),
                Some("Asks to read the staging db pane: the migration failed there"),
            ),
            (
                "Nightly canary",
                "RELAY \u{203a} nightly",
                "SOL 6 XHIGH",
                Some(K::Failure),
                false,
                inbox::Life::Awake,
                Some(6 * 60),
                Some("API Error: 429 rate_limit_error \u{b7} retry after 1m"),
            ),
            (
                "Theme contrast pass",
                "ATLAS \u{203a} theme-foundry",
                "SONNET 5.5",
                Some(K::ReviewReady),
                false,
                inbox::Life::Awake,
                Some(12 * 60),
                Some("Fourteen themes checked; two need a darker selection colour."),
            ),
            (
                "Export the quarter",
                "LEDGER \u{203a} exports",
                "OPUS 5.5 HIGH",
                None,
                false,
                inbox::Life::Awake,
                None,
                Some("Paused until you were back: the CSV is ready to look at."),
            ),
            (
                "Probe the old mirror",
                "RELAY",
                "CODEX",
                Some(K::Unknown),
                false,
                inbox::Life::Unknown,
                None,
                None,
            ),
            (
                "Schema migrations",
                "ATLAS \u{203a} migrations",
                "OPUS 5.5 MAX",
                None,
                true,
                inbox::Life::Awake,
                None,
                None,
            ),
            (
                "Dedupe the vendors",
                "LEDGER \u{203a} vendors",
                "SOL 6 HIGH",
                None,
                true,
                inbox::Life::Awake,
                None,
                None,
            ),
            (
                "Canary dashboards",
                "RELAY \u{203a} canary",
                "SONNET 5.5",
                None,
                true,
                inbox::Life::Awake,
                None,
                None,
            ),
            (
                "Draft the release notes",
                "ATLAS \u{203a} release",
                "OPUS 5.5 HIGH",
                Some(K::ReviewReady),
                false,
                inbox::Life::Awake,
                Some(3 * 60),
                None,
            ),
            (
                "Vendor contract review",
                "LEDGER \u{203a} legal",
                "OPUS 5.5 MAX",
                Some(K::Decision),
                false,
                inbox::Life::Awake,
                Some(30 * 60),
                None,
            ),
            (
                "Tidy the fixtures",
                "ATLAS \u{203a} tests",
                "SONNET 5.5",
                None,
                false,
                inbox::Life::Awake,
                None,
                None,
            ),
            (
                "Map the old cron jobs",
                "RELAY \u{203a} cron",
                "SOL 6 HIGH",
                None,
                false,
                inbox::Life::Resting,
                None,
                None,
            ),
            (
                "Font licence audit",
                "ATLAS \u{203a} fonts",
                "CODEX",
                None,
                false,
                inbox::Life::Asleep,
                None,
                None,
            ),
            (
                "Archive 2025 logs",
                "LEDGER \u{203a} archive",
                "SONNET 5.5",
                None,
                false,
                inbox::Life::Asleep,
                None,
                None,
            ),
            (
                "Bump the toolchain",
                "RELAY \u{203a} ci",
                "OPUS 5.5 HIGH",
                None,
                false,
                inbox::Life::Asleep,
                None,
                None,
            ),
            (
                "Fix the flaky upload test",
                "ATLAS \u{203a} uploads",
                "SOL 6 XHIGH",
                Some(K::ReviewReady),
                false,
                inbox::Life::Awake,
                Some(90 * 60),
                None,
            ),
            (
                "Invoice template copy",
                "LEDGER \u{203a} templates",
                "SONNET 5.5",
                None,
                false,
                inbox::Life::Awake,
                None,
                None,
            ),
        ];
        let mut facts = Vec::new();
        let mut panes = HashMap::new();
        for (i, (title, place, lead, lane, working, life, age, line)) in
            rows.into_iter().enumerate()
        {
            let pane = 900_000 + i as u64;
            if !live.is_empty() {
                panes.insert(pane, live[i % live.len()]);
            }
            facts.push(inbox::Fact {
                key: inbox::Key::Window(pane),
                pane,
                life,
                lane,
                working,
                priority: if i == 0 {
                    attention::Priority::Promoted
                } else {
                    attention::Priority::Neutral
                },
                changed: age.and_then(ago),
                evidence: line.filter(|_| lane.is_some()).map(str::to_string),
                title: title.to_string(),
                place: place.to_string(),
                lead: Some(lead.to_string()),
                line: line.map(str::to_string),
            });
        }
        (facts, panes)
    }

    /// The demo's person-state: a woken row, two snoozes, two done marks, the
    /// inbox showing and two shelves open — every shelf with something on it.
    fn inbox_demo_store() -> inbox::Store {
        let now = SystemTime::now();
        let at = Instant::now();
        let mut store = inbox::Store::default();
        store.set_showing(true);
        store.toggle_open(inbox::Shelf::Working);
        store.toggle_open(inbox::Shelf::Snoozed);
        store.woke.insert(
            inbox::Key::Window(900_004),
            inbox::Woke { at: now, from: at },
        );
        if let Some(t) = inbox::Wake::Morning.at(now) {
            store.snooze(inbox::Key::Window(900_009), t, now, at, None);
        }
        store.snooze(
            inbox::Key::Window(900_010),
            now + Duration::from_secs(2 * 3600),
            now,
            at,
            None,
        );
        store.mark_done(
            inbox::Key::Window(900_016),
            now - Duration::from_secs(3600),
            at,
            None,
        );
        store.mark_done(
            inbox::Key::Window(900_017),
            now - Duration::from_secs(600),
            at,
            None,
        );
        store.take_dirty();
        store
    }

    /// The inbox this session wrote down, or an empty one. A file that cannot
    /// be read is reported and set aside, never half-applied.
    pub(crate) fn inbox_load() -> inbox::Store {
        if inbox_demo() {
            return Self::inbox_demo_store();
        }
        let path = inbox::path_for(&instance::config_dir(), instance::key());
        match std::fs::read_to_string(&path) {
            Ok(text) => inbox::decode(&text, Instant::now()).unwrap_or_else(|e| {
                eprintln!("terminal-delight: inbox {} not read: {e}", path.display());
                inbox::Store::default()
            }),
            Err(_) => inbox::Store::default(),
        }
    }

    /// Go to a row's pane. The row counts as visited at once, so a WOKE mark
    /// does not outlive the click that answered it.
    pub(crate) fn inbox_go(&mut self, key: inbox::Key, pane: u64, cx: &mut Context<Self>) {
        let (_, panes) = self.inbox_rows(cx);
        let Some(id) = panes.get(&pane).copied() else {
            return;
        };
        self.inbox_store.borrow_mut().visited(key);
        self.inbox_snoozing = None;
        // Going to a pane with the composer up would leave the page over the
        // terminal the keyboard just went to: what is typed next would land
        // behind it. A row's click means "go there", so the page goes away.
        self.composer_put_away();
        self.pending_jump = Some(id);
        self.inbox_save();
        cx.notify();
    }

    /// The agent whose INBOX row is under `pos`, when it is not `from` and
    /// both are agents: what a release there would relate `from` to.
    pub(crate) fn inbox_relate_target(
        &self,
        from: EntityId,
        pos: Point<Pixels>,
        cx: &App,
    ) -> Option<Entity<TerminalView>> {
        let id = self
            .inbox_bounds
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(_, b)| b.contains(&pos))
            .map(|(id, _)| *id)
            .filter(|id| *id != from)?;
        let both = self.pane_by_id(from)?.read(cx).hosts_agent();
        self.pane_by_id(id)
            .filter(|to| both && to.read(cx).hosts_agent())
    }

    /// The drag chip over an agent's row: who the release would introduce.
    pub(crate) fn inbox_relate_chip(&self, to: &Entity<TerminalView>, cx: &App) -> String {
        let name = self.pane_name(to, cx);
        let name = if name.tab.is_empty() {
            name.full()
        } else {
            name.tab
        };
        lang::fill(self.lang.strings().entry.drag_relate, &[("name", &name)])
    }

    /// The row a drag in flight would land on, if any: lit so the drop is
    /// never a guess.
    fn inbox_drop_over(&self, cx: &App) -> Option<EntityId> {
        if let Some(d) = self.inbox_drag().filter(|d| d.engaged) {
            return self
                .inbox_relate_target(d.from, d.at, cx)
                .map(|e| e.entity_id());
        }
        let d = self.drag_pane().filter(|d| d.engaged)?;
        self.inbox_relate_target(d.id, d.at, cx)
            .map(|e| e.entity_id())
    }

    /// The rows a person can see, top to bottom: LOOK AT ME, then each open
    /// shelf. The keyboard walks exactly what is drawn, the rail's rule for its
    /// own cursor — "row 4" means the fourth row on screen or nothing.
    fn inbox_drawn(&self, cx: &App) -> Vec<(inbox::Key, u64)> {
        let (shelves, _) = self.inbox_rows(cx);
        let store = self.inbox_store.borrow();
        inbox::Shelf::ALL
            .into_iter()
            .filter(|s| store.is_open(*s))
            .flat_map(|s| shelves.shelf(s).iter().map(|r| (r.fact.key, r.fact.pane)))
            .collect()
    }

    /// Ctrl+Alt+↑/↓ on the INBOX face: move the highlight a row, wrapping, and
    /// starting from the top (or the bottom, going up) when there is none.
    pub(crate) fn inbox_walk(&mut self, down: bool, cx: &mut Context<Self>) {
        let drawn = self.inbox_drawn(cx);
        if drawn.is_empty() {
            return;
        }
        let n = drawn.len();
        let at = self
            .inbox_cursor
            .and_then(|k| drawn.iter().position(|(key, _)| *key == k));
        let next = match (at, down) {
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
            (None, true) => 0,
            (None, false) => n - 1,
        };
        self.inbox_cursor = Some(drawn[next].0);
        cx.notify();
    }

    /// Ctrl+Alt+→ on the INBOX face: go to the highlighted agent.
    pub(crate) fn inbox_enter(&mut self, cx: &mut Context<Self>) {
        let drawn = self.inbox_drawn(cx);
        if let Some(&(key, pane)) = self
            .inbox_cursor
            .and_then(|k| drawn.iter().find(|(key, _)| *key == k))
        {
            self.inbox_go(key, pane, cx);
        }
    }

    /// Ctrl+Alt+1…9 on the INBOX face: go straight to the nth drawn row.
    pub(crate) fn inbox_jump(&mut self, n: usize, cx: &mut Context<Self>) {
        let drawn = self.inbox_drawn(cx);
        if let Some(&(key, pane)) = n.checked_sub(1).and_then(|i| drawn.get(i)) {
            self.inbox_cursor = Some(key);
            self.inbox_go(key, pane, cx);
        }
    }

    fn inbox_snooze(&mut self, key: inbox::Key, until: SystemTime, cx: &mut Context<Self>) {
        self.inbox_hover = None;
        let seen = self.inbox_seen(key, cx);
        self.inbox_store
            .borrow_mut()
            .snooze(key, until, SystemTime::now(), Instant::now(), seen);
        self.inbox_snoozing = None;
        self.inbox_save();
        cx.notify();
    }

    fn inbox_done(&mut self, key: inbox::Key, cx: &mut Context<Self>) {
        self.inbox_hover = None;
        let seen = self.inbox_seen(key, cx);
        self.inbox_store
            .borrow_mut()
            .mark_done(key, SystemTime::now(), Instant::now(), seen);
        self.inbox_snoozing = None;
        self.inbox_save();
        cx.notify();
    }

    /// The line the row was quoting when the person acted on it: a question
    /// with different words after that is a new question, even in the same
    /// lane (see [`inbox::Store::advance`]).
    fn inbox_seen(&self, key: inbox::Key, cx: &App) -> Option<String> {
        let (shelves, _) = self.inbox_rows(cx);
        let seen = shelves
            .rows()
            .find(|r| r.fact.key == key)
            .and_then(|r| r.fact.evidence.clone());
        seen
    }

    fn inbox_wake(&mut self, key: inbox::Key, cx: &mut Context<Self>) {
        self.inbox_hover = None;
        self.inbox_store
            .borrow_mut()
            .wake(key, SystemTime::now(), Instant::now());
        self.inbox_save();
        cx.notify();
    }

    fn inbox_reopen(&mut self, key: inbox::Key, cx: &mut Context<Self>) {
        self.inbox_hover = None;
        self.inbox_store.borrow_mut().reopen(key);
        self.inbox_save();
        cx.notify();
    }

    fn inbox_fold(&mut self, shelf: inbox::Shelf, cx: &mut Context<Self>) {
        self.inbox_store.borrow_mut().toggle_open(shelf);
        self.inbox_save();
        cx.notify();
    }

    /// A face button of the spine header: a bezel whose label gives way with
    /// an ellipsis when the row is short, rather than running under the ◀ at
    /// the row's end (chalk's wider letters, and Spanish, French or Russian in
    /// any skin, did).
    ///
    /// The face you are on keeps its whole label (`flex_none`); only the other
    /// one gives way, and only its word: a count never does, because it is
    /// what the face is there to say (the PROJECTS face's ▼ squeezed INBOX 5
    /// to "INBOX…").
    fn spine_face_btn(
        sk: &skin::Skin,
        label: &str,
        count: Option<usize>,
        active: bool,
        s: f32,
    ) -> gpui::Div {
        sk.bezel(active, s)
            .when(active, |d| d.flex_none())
            // Narrower sides than a bezel's own: the row holds two faces, a +
            // and the ◀ in 208 pixels, and a wide face (chalk's handwriting)
            // would otherwise give up most of its letters to the padding.
            .px(px(5. * s))
            .min_w_0()
            .overflow_hidden()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(3. * s))
            .child(div().min_w_0().truncate().child(sk.control_text(label)))
            .children(count.map(|n| div().flex_none().child(n.to_string())))
    }

    /// The spine header's two faces, PROJECTS and INBOX, the second carrying
    /// how many rows want the person — the same count, by the same rule, as
    /// the number on the closed rail.
    pub(crate) fn render_spine_faces(
        &self,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let showing = self.spine_inbox();
        let (shelves, _) = self.inbox_rows(cx);
        let wanting = shelves.wanting();
        let e = self.lang.strings().entry;
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(3. * s))
            // Each face says its whole name on hover, since a short row
            // shortens the one you are not on.
            .child(with_tip(
                Self::spine_face_btn(sk, e.spine_projects, None, !showing, s * 0.8)
                    .id("spine-face-projects")
                    .debug_selector(|| "spine-face-projects".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.set_spine_inbox(false, cx);
                        }),
                    ),
                vec![e.spine_projects.to_string()],
            ))
            .child(
                Self::spine_face_btn(
                    sk,
                    e.spine_inbox,
                    (wanting > 0).then_some(wanting),
                    showing,
                    s * 0.8,
                )
                .id("spine-face-inbox")
                .debug_selector(|| "spine-face-inbox".into())
                .when(wanting > 0 && !showing, |d| d.text_color(sk.ink.warn))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.set_spine_inbox(true, cx);
                    }),
                )
                .map(|d| {
                    let name = match wanting {
                        0 => e.spine_inbox.to_string(),
                        n => format!("{} {n}", e.spine_inbox),
                    };
                    with_tip(d, vec![name])
                }),
            )
    }

    /// The inbox's rows, top shelf first, for the spine's list.
    pub(crate) fn inbox_list(
        &self,
        th: &theme::Theme,
        s: f32,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let sk = skin::skin(cx, s);
        let (shelves, panes) = self.inbox_rows(cx);
        // Read against last frame's boxes, then rebuilt from this frame's.
        let over = self.inbox_drop_over(cx);
        self.inbox_bounds.lock().unwrap().clear();
        let e = self.lang.strings().entry;
        let mut out: Vec<AnyElement> = Vec::new();
        if shelves.is_empty() {
            out.push(
                div()
                    .px(px(8. * s))
                    .py(px(10. * s))
                    .text_size(sk.text_px(CHROME_NAME_PT * s))
                    .text_color(sk.ink.ink_dim)
                    .child(e.inbox_empty)
                    .into_any_element(),
            );
            return out;
        }
        out.push(self.inbox_shelf_header(inbox::Shelf::LookAtMe, shelves.wanting(), &sk, s, cx));
        if shelves.look.is_empty() {
            out.push(
                div()
                    .px(px(10. * s))
                    .py(px(6. * s))
                    .text_size(sk.text_px(CHROME_NAME_PT * s * 0.95))
                    .text_color(sk.ink.ink_dim)
                    .child(e.look_empty)
                    .into_any_element(),
            );
        }
        let mut n = 0usize;
        for row in &shelves.look {
            out.push(self.inbox_row(
                row,
                n,
                {
                    let id = panes.get(&row.fact.pane).copied();
                    (id, id.is_some() && over == id)
                },
                &sk,
                th,
                s,
                cx,
            ));
            n += 1;
        }
        // The folded shelves sit at the foot, the way T3 parks its working,
        // snoozed and settled shelves under the active list: what wants you
        // at the top, what does not out of the way.
        out.push(div().flex_1().min_h(px(10. * s)).into_any_element());
        for shelf in [
            inbox::Shelf::Working,
            inbox::Shelf::Snoozed,
            inbox::Shelf::Quiet,
            inbox::Shelf::Done,
        ] {
            let rows = shelves.shelf(shelf);
            if rows.is_empty() {
                continue;
            }
            out.push(self.inbox_shelf_header(shelf, rows.len(), &sk, s, cx));
            if self.inbox_store.borrow().is_open(shelf) {
                for row in rows {
                    out.push(self.inbox_row(
                        row,
                        n,
                        {
                            let id = panes.get(&row.fact.pane).copied();
                            (id, id.is_some() && over == id)
                        },
                        &sk,
                        th,
                        s,
                        cx,
                    ));
                    n += 1;
                }
            }
        }
        out
    }

    /// A shelf's name and count. The folding ones fold on a press; LOOK AT ME
    /// does not fold and carries no triangle.
    fn inbox_shelf_header(
        &self,
        shelf: inbox::Shelf,
        count: usize,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let e = self.lang.strings().entry;
        let name = match shelf {
            inbox::Shelf::LookAtMe => e.shelf_look,
            inbox::Shelf::Working => e.shelf_working,
            inbox::Shelf::Snoozed => e.shelf_snoozed,
            inbox::Shelf::Quiet => e.shelf_quiet,
            inbox::Shelf::Done => e.shelf_done,
        };
        let open = self.inbox_store.borrow().is_open(shelf);
        let loud = shelf == inbox::Shelf::LookAtMe && count > 0;
        let ink = if loud { sk.ink.warn } else { sk.ink.ink_dim };
        let id = SharedString::from(format!("inbox-shelf-{shelf:?}"));
        div()
            .id(id)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(5. * s))
            .px(px(6. * s))
            .pt(px(6. * s))
            .pb(px(3. * s))
            .when(shelf.folds(), |d| {
                d.cursor_pointer()
                    .hover(move |st| st.bg(hsla(0., 0., 1., 0.05)))
                    .child(Self::triangle(
                        if open { BarDir::Down } else { BarDir::Right },
                        sk.ink.ink_dim,
                        s,
                        sk,
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.inbox_fold(shelf, cx);
                        }),
                    )
            })
            .child(
                div()
                    .flex_none()
                    .text_size(sk.text_px(CHROME_NAME_PT * s * 0.92))
                    .text_color(ink)
                    .child(name),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(sk.text_px(CHROME_NAME_PT * s * 0.85))
                    .text_color(if loud { sk.ink.warn } else { sk.ink.ink_faint })
                    .when(count > 0 || shelf.folds(), |d| d.child(count.to_string())),
            )
            .child(div().h(px(1.)).flex_1().bg(sk.ink.rule))
            // LOOK AT ME says which order it is in, and a press switches. The
            // order is the one call in here left open for Parker to make, so
            // both are a press apart rather than one of them hidden.
            .when(shelf == inbox::Shelf::LookAtMe && count > 1, |d| {
                let order = self.inbox_store.borrow().order.unwrap_or_default();
                d.child(
                    div()
                        .id("inbox-look-order")
                        .debug_selector(|| "inbox-look-order".into())
                        .flex_none()
                        .px(px(3. * s))
                        .rounded(sk.radius())
                        .cursor_pointer()
                        .hover(|st| st.bg(hsla(0., 0., 1., 0.08)))
                        .text_size(sk.text_px(CHROME_NAME_PT * s * 0.72))
                        .text_color(sk.ink.ink_faint)
                        .child(match order {
                            inbox::LookOrder::Rail => e.order_rail,
                            inbox::LookOrder::Returned => e.order_returned,
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.inbox_store.borrow_mut().set_order(match order {
                                    inbox::LookOrder::Rail => inbox::LookOrder::Returned,
                                    inbox::LookOrder::Returned => inbox::LookOrder::Rail,
                                });
                                ws.inbox_save();
                                cx.notify();
                            }),
                        ),
                )
            })
            .into_any_element()
    }

    /// The mark at the front of a row: the robot the tab strip wears for the
    /// same state where there is one, so one state has one face everywhere.
    fn inbox_mark(
        &self,
        row: &inbox::Row,
        n: usize,
        sk: &skin::Skin,
        th: &theme::Theme,
        s: f32,
    ) -> AnyElement {
        use attention::AttentionKind as K;
        let badge = match row.status {
            inbox::Status::Lane(K::Decision) => Some(AgentBadge::NeedsInput),
            inbox::Status::Lane(K::Failure) => Some(AgentBadge::Blocked),
            inbox::Status::Lane(K::ReviewReady) => Some(AgentBadge::Done),
            inbox::Status::Working => Some(AgentBadge::Working),
            _ => None,
        };
        if let Some(badge) = badge {
            return Self::agent_badge_el(badge, INBOX_BADGE_KEYS + n, 0, s * 0.85, sk);
        }
        let (glyph, ink) = match row.status {
            inbox::Status::Lane(kind) => (
                if kind == K::Access { "\u{26bf}" } else { "?" },
                Self::rail_ink(kind, None, th, sk),
            ),
            inbox::Status::Woke => ("\u{2600}", sk.ink.warn),
            inbox::Status::Snoozed => ("\u{263e}", sk.ink.ink_dim),
            inbox::Status::Done => ("\u{2713}", sk.ink.ok),
            inbox::Status::Resting => ("\u{2744}", sk.ink.ink_dim),
            inbox::Status::Asleep => ("z", sk.ink.ink_dim),
            _ => ("\u{b7}", sk.ink.ink_dim),
        };
        div()
            .flex_none()
            .w(px(14. * s))
            .flex()
            .justify_center()
            .text_size(sk.text_px(11. * s))
            .text_color(ink)
            .child(glyph)
            .into_any_element()
    }

    /// What a row says on the right of its title: how long it has waited, or
    /// the person's own date for it, or the state word.
    fn inbox_when(&self, row: &inbox::Row) -> Option<String> {
        let l = self.lang;
        let e = l.strings().entry;
        let p = l.strings().parts;
        let now = SystemTime::now();
        match row.status {
            inbox::Status::Lane(_) => Some(attention::age_label_in(
                l,
                row.fact
                    .changed
                    .map(|c| Instant::now().saturating_duration_since(c)),
            )),
            inbox::Status::Woke => Some(e.status_woke.to_string()),
            inbox::Status::Working => None,
            inbox::Status::Idle => Some(p.hud_state_idle.to_string()),
            inbox::Status::Resting => Some(p.hud_state_resting.to_string()),
            inbox::Status::Asleep => Some(p.hud_state_asleep.to_string()),
            inbox::Status::Snoozed => row
                .when
                .and_then(|t| handover::card_time(ms(t), ms(now), l))
                .map(|w| lang::fill(e.status_until, &[("when", &w)])),
            inbox::Status::Done => row
                .when
                .and_then(|t| handover::card_time(ms(t), ms(now), l))
                .map(|w| lang::fill(e.status_done_at, &[("when", &w)])),
        }
    }

    /// One row: mark, title and when on the first line, where it lives on the
    /// second, the agent's own words on a third when the row wants you. The
    /// verbs appear under the pointer; a click anywhere else goes to the pane.
    #[allow(clippy::too_many_arguments)]
    fn inbox_row(
        &self,
        row: &inbox::Row,
        n: usize,
        // The pane the row stands for, and whether a drag would land on it.
        (id, lit): (Option<EntityId>, bool),
        sk: &skin::Skin,
        th: &theme::Theme,
        s: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let e = self.lang.strings().entry;
        let key = row.fact.key;
        let pane = row.fact.pane;
        let wants = row.shelf == inbox::Shelf::LookAtMe;
        let snoozing = self.inbox_snoozing == Some(key);
        // The pane you are in, marked the way the tree marks the active task:
        // the list answers "where am I" as well as "what wants me".
        let here = self
            .tabs
            .get(self.active)
            .and_then(|t| t.focused)
            .is_some_and(|id| id.as_u64() == pane);
        let title_ink = if wants {
            sk.ink.ink_lit
        } else {
            th.text.alpha(0.78)
        };
        let place = match (&row.fact.place, &row.fact.lead) {
            (p, Some(lead)) if !p.is_empty() => format!("{p} \u{b7} {lead}"),
            (p, None) => p.clone(),
            (_, Some(lead)) => lead.clone(),
        };

        // The verbs for this shelf, as (id, glyph, word, what it does).
        #[derive(Clone, Copy)]
        enum Verb {
            Snooze,
            Done,
            Wake,
            Reopen,
        }
        let verbs: &[Verb] = match row.shelf {
            inbox::Shelf::Snoozed => &[Verb::Wake, Verb::Done],
            inbox::Shelf::Done => &[Verb::Reopen],
            _ => &[Verb::Snooze, Verb::Done],
        };
        // The verbs float over the row's right-hand end, and only on the row
        // the pointer is over (or the keyboard's): hidden buttons that still
        // took clicks would turn a press meant for the row into a snooze, and
        // reserved space for them would cut every title short.
        let shows_verbs = self.inbox_hover == Some(key) || self.inbox_cursor == Some(key);
        let mut actions = div()
            .absolute()
            .top(px(2. * s))
            .right(px(3. * s))
            .flex()
            .flex_row()
            .gap(px(2. * s))
            .px(px(2. * s))
            .rounded(sk.radius())
            .bg(th.surface.alpha(0.96))
            .border(sk.box_border(1.))
            .border_color(sk.ink.edge_rest);
        for (vi, verb) in verbs.iter().copied().enumerate() {
            let (glyph, word) = match verb {
                Verb::Snooze => ("\u{263e}", e.act_snooze),
                Verb::Done => ("\u{2713}", e.act_done),
                Verb::Wake => ("\u{2600}", e.act_wake),
                Verb::Reopen => ("\u{21ba}", e.act_reopen),
            };
            actions = actions.child(
                div()
                    .id(SharedString::from(format!("inbox-act-{n}-{vi}")))
                    .debug_selector(move || format!("inbox-act-{n}-{vi}"))
                    .px(px(3. * s))
                    .rounded(sk.radius())
                    .text_size(sk.text_px(11. * s))
                    .text_color(th.text.alpha(0.85))
                    .hover(|st| st.bg(hsla(0., 0., 1., 0.10)))
                    .cursor_pointer()
                    .child(SharedString::from(sk.control(glyph, word)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            match verb {
                                Verb::Snooze => {
                                    ws.inbox_snoozing =
                                        (ws.inbox_snoozing != Some(key)).then_some(key);
                                    cx.notify();
                                }
                                Verb::Done => ws.inbox_done(key, cx),
                                Verb::Wake => ws.inbox_wake(key, cx),
                                Verb::Reopen => ws.inbox_reopen(key, cx),
                            }
                        }),
                    ),
            );
        }

        let mut body = div()
            .id(SharedString::from(format!("inbox-row-{n}")))
            .debug_selector(move || format!("inbox-row-{n}"))
            .relative()
            .mx(px(3. * s))
            .px(px(5. * s))
            .py(px(3. * s))
            .flex()
            .flex_col()
            .gap(px(1. * s))
            .rounded(sk.radius())
            .cursor_pointer()
            .hover(|st| st.bg(hsla(0., 0., 1., 0.06)))
            .when(row.woke, |d| d.bg(sk.ink.warn.alpha(0.10)))
            .when(here, |d| {
                d.child(
                    div()
                        .absolute()
                        .left(px(0.))
                        .top(px(4. * s))
                        .bottom(px(4. * s))
                        .w(px(2. * s))
                        .rounded(sk.radius())
                        .bg(sk.ink.select),
                )
            })
            // The keyboard's highlight: an inset ring and a tint, never a
            // border, so a row does not grow a pixel under the cursor — the
            // tree's own cursor ring, for the same reason.
            .when(self.inbox_cursor == Some(key), |d| {
                d.bg(th.cursor.alpha(0.14)).shadow(vec![BoxShadow {
                    color: th.cursor.alpha(0.85),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(1.),
                    inset: true,
                }])
            })
            .on_hover(cx.listener(move |ws, hovered: &bool, _w, cx| {
                if *hovered {
                    ws.inbox_hover = Some(key);
                } else if ws.inbox_hover == Some(key) {
                    ws.inbox_hover = None;
                }
                cx.notify();
            }))
            // The title has the whole first line: in a 208-pixel spine every
            // glyph of it is worth more than an age, which reads fine below.
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(5. * s))
                    .child(self.inbox_mark(row, n, sk, th, s))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(sk.text_px(CHROME_NAME_PT * s))
                            .text_color(title_ink)
                            .child(row.fact.title.clone()),
                    ),
            )
            .child(
                div()
                    .pl(px(19. * s))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6. * s))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(sk.text_px(CHROME_NAME_PT * s * 0.8))
                            .text_color(sk.ink.ink_faint)
                            .child(place),
                    )
                    .children(self.inbox_when(row).map(|w| {
                        div()
                            .flex_none()
                            .text_size(sk.text_px(CHROME_NAME_PT * s * 0.8))
                            .text_color(if row.woke {
                                sk.ink.warn
                            } else {
                                sk.ink.ink_faint
                            })
                            .child(w)
                    })),
            )
            .when_some(row.fact.line.clone().filter(|_| wants), |d, line| {
                d.child(
                    div()
                        .pl(px(19. * s))
                        .truncate()
                        .text_size(sk.text_px(CHROME_NAME_PT * s * 0.82))
                        .text_color(th.text.alpha(0.55))
                        .child(line),
                )
            })
            .when(shows_verbs, |d| d.child(actions))
            // The row a drag would land on, lit in the accent the drag chip
            // wears, so a drop names who it introduces before it happens.
            .when(lit, |d| {
                d.bg(th.accent.alpha(0.18)).shadow(vec![BoxShadow {
                    color: th.accent.alpha(0.9),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(1.),
                    inset: true,
                }])
            })
            .when_some(id, |d, id| {
                let store = self.inbox_bounds.clone();
                d.child(
                    div().absolute().inset_0().child(
                        canvas(
                            move |bounds, _, _| store.lock().unwrap().push((id, bounds)),
                            |_, _, _, _| {},
                        )
                        .size_full(),
                    ),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, ev: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    ws.inbox_go(key, pane, cx);
                    // A press is a click; carried past the threshold it is a
                    // drag toward another agent's row.
                    ws.set_inbox_drag(id.map(|from| InboxDrag {
                        from,
                        start: ev.position,
                        at: ev.position,
                        engaged: false,
                    }));
                }),
            );

        if snoozing {
            body = body.child(self.inbox_snooze_menu(key, n, sk, s, cx));
        }
        body.into_any_element()
    }

    /// The snooze choices, opened under the row that asked for them rather than
    /// floated over the panes: a menu over the glass would need the warp
    /// flattened for it, and one inside the spine is never over a tube.
    fn inbox_snooze_menu(
        &self,
        key: inbox::Key,
        n: usize,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let l = self.lang;
        let e = l.strings().entry;
        let now = SystemTime::now();
        let mut menu = div()
            .mt(px(3. * s))
            .ml(px(19. * s))
            .flex()
            .flex_col()
            .gap(px(2. * s))
            .child(
                div()
                    .text_size(sk.text_px(CHROME_NAME_PT * s * 0.8))
                    .text_color(sk.ink.ink_faint)
                    .child(e.snooze_title),
            );
        for (wi, (wake, until)) in inbox::Wake::menu(now).into_iter().enumerate() {
            let word = match wake {
                inbox::Wake::Hour => e.wake_hour,
                inbox::Wake::ThreeHours => e.wake_three,
                inbox::Wake::Evening => e.wake_evening,
                inbox::Wake::Morning => e.wake_morning,
                inbox::Wake::Monday => e.wake_monday,
            };
            let when = handover::card_time(ms(until), ms(now), l).unwrap_or_default();
            menu = menu.child(
                sk.chip(false)
                    .id(SharedString::from(format!("inbox-snooze-{n}-{wi}")))
                    .debug_selector(move || format!("inbox-snooze-{n}-{wi}"))
                    .cursor_pointer()
                    .text_size(sk.text_px(CHROME_NAME_PT * s * 0.85))
                    .child(format!("{word} \u{b7} {when}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.inbox_snooze(key, until, cx);
                        }),
                    ),
            );
        }
        menu
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_loses_the_agents_status_glyph_and_keeps_its_words() {
        assert_eq!(bare_title("✳ T3 code on farbox"), "T3 code on farbox");
        assert_eq!(
            bare_title("◐ Using expiring credits"),
            "Using expiring credits"
        );
        assert_eq!(bare_title("[ ! ] Action Required"), "[ ! ] Action Required");
        assert_eq!(bare_title("shell"), "shell");
        assert_eq!(bare_title("parker@legion:~/.cache/td"), "");
        assert_eq!(bare_title("me@box:/srv/app"), "");
        assert_eq!(bare_title("Reply to me@box: done"), "Reply to me@box: done");
        assert_eq!(bare_title("ssh me@box"), "ssh me@box");
    }

    #[test]
    fn one_line_takes_the_first_line_and_cuts_at_a_word() {
        assert_eq!(one_line("\n  first   line \nsecond", 40), "first line");
        assert_eq!(
            one_line("the quick brown fox jumps over the lazy dog", 20),
            "the quick brown fox\u{2026}"
        );
        assert_eq!(one_line("", 10), "");
    }

    /// The spine is one surface with two faces: the inbox is drawn INSIDE the
    /// left bar's own list, never as a panel of its own over the panes, so it
    /// can neither resize a terminal nor need the glass flattened for it.
    #[test]
    fn the_inbox_is_drawn_inside_the_spines_list() {
        let src = include_str!("main.rs");
        let at = src
            .find("    fn render_left_bar(")
            .expect("render_left_bar");
        let end = src[at..].find("\n    }\n").expect("end of render_left_bar");
        let bar = &src[at..at + end];
        assert!(bar.contains("self.inbox_list("), "the list swaps faces");
        assert!(
            bar.contains("self.render_spine_faces("),
            "the header carries both faces"
        );
        for id in ["bar-fold-all", "bar-new-project", "bar-hide"] {
            assert!(bar.contains(id), "the actions row lost {id}");
        }
    }
}
