# 03 — Program design: Client-server split (Gate 3)

Drafted 2026-09-08 by a four-domain fleet (host+wire, attach seam+gridwire,
GUI attach+persistence, slice-0+harness), integrated, then adversarially
verified by two passes — existence (every cited symbol checked against the
live tree and the vendored alacritty 0.26 source; 22 checks) and coverage
(every binding decision and Gate 2 graft traced into the design and its test
plan; 23 checks). All six not-ok findings are already folded in; the
substantive one — the snapshot encoder's original alt-screen mechanism would
have erased a running vim's live grid, falsified at term/mod.rs:732 — is
redesigned as a read-only alt paint healed by a fenced re-snapshot on alt
exit (see encode_snapshot and least-confident decision 9). Panel record:
research-gate3-panel.md.

## Decision round — answered by Parker, 2026-09-08

**APPROVED** — "Fully concur all in the doc", with building authorised to
continue while he is away ("Go full send").

1. **Gate 3 approved.** The signatures stand as written, corrections folded.
2. **The alt-screen attach trade: accepted for v1.** Reattaching to a pane
   sitting in vim shows vim immediately and perfectly; the scrollback behind
   it stays empty until vim exits, then heals in full. Nothing is ever lost
   host-side. Forking the vendored terminal crate to expose the hidden grid
   stays available if the heal feels bad in practice.
3. **The window that loses a steal freezes** in v1 — no new UI, per the
   product gate. Auto-close is a one-line policy change and gets judged in
   the flesh at the Gate 4 demo (least-confident decision 13).
4. **Straight into the slice plan.** 04-slices.md, then slice 0 alone.

> Types and signatures only — no bodies. Integrated from four domain drafts
> (session-host-and-wire, attach-seam-and-gridwire, GUI-attach-and-persistence,
> slice-0-and-proof-harness) against the approved `02-architecture.md` and its
> answered decision round. Every signature that builds on existing code cites
> the live tree at `~/Work/terminal-delight` or the vendored
> `alacritty_terminal-0.26.0` crate source; all cited anchors were re-verified
> 2026-09-08 (`MAX_PANES` main.rs:75, `persist_primary_state` main.rs:1250,
> `SavedNode` main.rs:431, `flag_reply` main.rs:18320, `main` main.rs:18333,
> pinning test main.rs:17173, `Session` term.rs:60, `spawn_in` term.rs:105,
> `resolve_session` instance.rs:299, `claim_in` instance.rs:471, `bind`
> instance.rs:86, `release` instance.rs:98, `write_atomic` session.rs:74,
> `capture` session.rs:59, `ctl_dir` ctl.rs:313, `new_restored` pane.rs:2455,
> `ApplyOutcome` mcp.rs:318).

## Files

Every file created or changed. One binary throughout; no crate split.

- `app/src/main.rs` — **modified.** Slice 0: the dispatch allowlist (`Launch`/`Verb`/`dispatch`) replacing the if-ladder at main.rs:18337-18398, and the rewrite of the pinning test at main.rs:17173, in the same commit. Later slices: `MAX_PANES` 8→4 plus `LEGACY_PANE_CEILING` 8, `SavedNode::Leaf.pane_id`, `Boot`/`resolve_boot`, `AttachCtx`/`plan_attach`/`build_attached`, save routing, `persist_primary_state` thinned to a wrapper, module registrations.
- `app/src/serve.rs` — **new.** The `serve` verb's permanent entry (`run_cli`, `parse_args`), so the allowlist has a real target at slice 0; the slice-0 body is a refusal (exit 2 naming the later slice), never a GUI.
- `app/src/hostproto.rs` — **new.** The one wire module, shared by host, client, and conformance tests: hello/version handshake, requests/replies/pushes, `PaneId`, `WireMode`, `probe_host`, `host_socket_path`. Pure serde+std. *(Resolution: two drafts spelled this `proto.rs` and `hostproto.rs`; one module, `hostproto.rs`, absorbs both — the handshake types and the verb types are one contract.)*
- `app/src/host.rs` — **new.** The session host runtime: `Host`, `HostPane`, accept loop with SO_PEERCRED, verb handlers, splice-on-stream-connect, watcher/checkpoint/guard threads, `run_serve`. gpui-free by module discipline, std threads like ctl.rs.
- `app/src/hostctl.rs` — **new.** GUI-side client of `session-<id>.sock`: `HostHandle` (attach, list, spawn, stream, save, events), `spawn_host`. std+libc only, testable without a window.
- `app/src/gridwire.rs` — **new.** Named by 02-architecture.md: the attach-time snapshot encoder, grid hash, the lease fence, the tee-sink registry, the divergence guard, and their property tests.
- `app/src/term.rs` — **modified.** The alacritty-facing seam file: `spawn_in` (term.rs:105) split into `spawn_pty` + `wire_event_loop` (signature of `spawn_in` unchanged, never deleted); `spawn_hosted` and `attach_in` land beside it; `SocketPty`/`CountingReader` (client adapter) and `TeePty`/`TeeReader` (host adapter) live here because this is already the one module written against the crate's public API; `EventProxy` becomes a named struct with `suppress_pty_write`; the `#[ignore]` echo-latency bench joins the inline test module.
- `app/src/instance.rs` — **modified.** `resolve_session_hosted` adds the first-ranked live-host tier beside the untouched `resolve_session` (instance.rs:299); `claim_in`/`release` stay verbatim for the host and legacy paths.
- `app/src/session.rs` — **modified.** `persist_primary` + `PersistOutcome` + `saved_counts` relocated beside `write_atomic` (session.rs:74) so the shrink guard is gpui-free and the host is the single TOML writer running today's exact guard.
- `app/src/pane.rs` — **modified.** `TerminalView::new_attached` beside `new_restored` (pane.rs:2455); `pane_id` field; `set_host_mode`; the 800ms /proc watcher is not spawned when `Session.master` is `None`.
- `app/src/ctl.rs` — **modified.** `ctl_dir()` (ctl.rs:313) becomes `pub(crate)` so `hostproto::host_socket_path` shares the one runtime-dir answer; `stamped_seat` (TD_SESSION+TD_PANE_ID) consulted before the /proc parent-walk in `owning_td` (ctl.rs:1031); session-alias symlink published by the attached window; `TabRef::Durable`.
- `app/Cargo.toml` — **modified.** Adds `polling = "3"` — implementing `EventedReadWrite` requires naming `polling::{Poller, Event, PollMode}` (vendored tty/mod.rs:72-74); Cargo.lock already resolves polling 3.11.0 transitively.
- `app/tests/dispatch_cli.rs` — **new.** End-to-end exit-code and no-disk-writes pins need the real binary (`CARGO_BIN_EXE`); first integration-test target in the crate.
- `docs/protocol/session-host-v1.md` — **new.** The one-page versioned contract; conformance tests deserialize its embedded JSON examples, so the document, not the implementation, is the authority.
- `scripts/td-survival-test.sh` — **new.** The metric instrument (lost sessions = 0), built at slice 0 per the panel graft so it exists before the behaviour it measures.
- `scripts/td-echo-bench.sh` — **new.** Flip-gate latency wrapper: runs the bench in both modes and both flood conditions, applies the ≤1ms jq gate, prints the numbers Parker signs.

## Types & signatures

### app/src/hostproto.rs (new) — the wire

NDJSON: one serde_json line per Request/Reply/Push on the control connection;
the byte-stream connection is raw bytes after one text line. Pure serde+std so
host, client, and tests share it.

```rust
pub const PROTO_VERSION: u32 = 1;
pub const ENV_SESSION: &str = "TD_SESSION";   // stamped into every host-spawned PTY
pub const ENV_PANE_ID: &str = "TD_PANE_ID";   // via vendored tty::Options.env (tty/mod.rs:36)

/// Durable, host-minted pane identity. Shell pid is an attribute, never an
/// address (Gate 2 answer 2; shrinks the #311/#299/#272/#151 class).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PaneId(pub u64);

/// session-<key>.sock in the same 0700 runtime dir as ctl-<pid>.sock
/// (ctl.rs ctl_dir(), :313 — made pub(crate) so both modules share one answer).
pub fn host_socket_path(key: &str) -> PathBuf;

/// Wire-side pane mode; classification logic mirrored from pane.rs
/// foreground_mode (:203-217) + PaneMode::classify (:41-63), std-only so the
/// host never imports pane.rs.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum WireMode { Shell, Claude, Codex, Remote, Other(String) }

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct PaneGeom { pub cols: u16, pub rows: u16, pub cell_width: u16, pub cell_height: u16 }

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum ClientKind {
    /// A window attaching. A second Gui hello STEALS: the host drops the
    /// previous Gui connection (decision 1) — never a refusal.
    Gui,
    /// ctl / scripts / the survival harness — attach-neutral, never steals.
    Cli,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "verb", rename_all = "kebab-case")]
pub enum Request {
    /// First line of every control connection. `build` (CARGO_PKG_VERSION) is
    /// for logs/status only and NEVER gates: `proto` is the sole compatibility fact.
    Hello { proto: u32, kind: ClientKind, build: String },
    ListPanes,
    /// Host spawns AND types the resume line (session::PaneRestore recipe,
    /// session.rs:27, executed host-side), stamping TD_SESSION+TD_PANE_ID.
    SpawnPane { cwd: Option<String>, resume: Option<String>, geom: PaneGeom },
    /// Declares intent + applies size; the atomic splice happens on the
    /// byte-stream connect (see host.rs).
    AttachPane { pane: PaneId, geom: PaneGeom },
    Resize { pane: PaneId, geom: PaneGeom },      // last-writer-wins; never on the byte stream
    ClosePane { pane: PaneId },                   // intent: SIGHUP the child tree
    /// Divergence-guard repair: RIS + fresh snapshot into the surviving sink.
    Resnapshot { pane: PaneId },
    /// Layout body is an OPAQUE schema-versioned envelope (client-owned data,
    /// #319-safe): the host merges only per-leaf cwd/resume via toml::Value,
    /// never a StateFile round-trip. No counts on the wire — the host recounts
    /// leaves/tabs from its own walk of the parsed envelope, one authority.
    /// (Resolution: one draft passed client counts, one recounted; recount wins
    /// so the shrink guard cannot be misfed by a lying or skewed client.)
    Save { schema: u32, body: String, allow_shrink: bool },
    Shutdown,                                     // checkpoint, then exit — the version-break degrade path
}

/// Truthful per-target outcome — the UiReq::Apply model (mcp_transport.rs:55,
/// mcp.rs:318 `pub type ApplyOutcome = (Target, Result<GradeReport, String>)`)
/// on the new surface.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Outcome<T> { Ok(T), Err(String) }

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PaneInfo {
    pub pane: PaneId,
    pub shell_pid: u32,           // attribute (term.rs:68), not the address
    pub cwd: Option<String>,      // None = not yet captured — unknown is not ""
    pub resume: Option<String>,   // None = no derivable agent session (session.rs:176 chain)
    pub mode: WireMode,
    pub attached: bool,           // a byte-stream sink currently holds this pane
    pub exited: bool,
    pub geom: PaneGeom,
    pub generation: u64,          // the invalidation token (term.rs:69-73)
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "reply", rename_all = "kebab-case")]
pub enum Reply {
    Hello { proto: u32, session: String, panes: usize, attached_gui: bool, build: String },
    /// Incompatible. Client's mandated next move: send Shutdown (host
    /// checkpoints, exits), then today's TOML recovery — degrades to exactly
    /// today, once, deliberately.
    VersionMismatch { host: u32, client: u32 },
    Panes { panes: Vec<PaneInfo> },
    Spawned { outcome: Outcome<PaneInfo> },
    Attached { pane: PaneId, outcome: Outcome<PaneInfo> },
    Resized { pane: PaneId, outcome: Outcome<()> },
    Closed { pane: PaneId, outcome: Outcome<CloseInfo> },
    /// Ok payload = the sink's enqueued-byte total at injection (offset checks restart there).
    Resnapshotted { pane: PaneId, outcome: Outcome<u64> },
    /// Ok payload = the relocated shrink guard's own verdict (session.rs); io
    /// failures ride Outcome::Err. (Resolution: three drafts had three persist
    /// outcome types — SaveInfo, SaveOutcome, PersistOutcome; the one type is
    /// session::PersistOutcome, serialized here directly.)
    Saved { outcome: Outcome<crate::session::PersistOutcome> },
    ShuttingDown,
    Error { msg: String },        // unknown verb / parse error — NEVER a fall-through
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CloseInfo { pub shell_pid: u32, pub signalled: bool }

/// Host -> client pushes, interleaved on the control connection.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum Push {
    Mode { pane: PaneId, mode: WireMode },        // the relocated 800ms watcher's output
    Exit { pane: PaneId, status: Option<i32> },   // Event::ChildExit (vendored event_loop.rs:263)
    Detached { pane: PaneId, reason: DetachReason },
    GridCheck(crate::gridwire::GridCheck),        // divergence-guard probe
}
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum DetachReason {
    Superseded,     // steal: a newer attach won
    SlowConsumer,   // sink overflow: this client must re-attach for a fresh fenced snapshot
}

/// The one compatibility rule, shared by both ends and the conformance tests:
/// equal major ⇒ Ok, else the refusal naming both. `build` never gates.
pub fn version_check(client: u32, host: u32) -> Result<(), Reply>;

/// Probe outcome. Unknown is not zero: a socket that exists but will not
/// answer is Unresponsive, never "no host". (Resolution: one draft returned
/// Option<HostProbe>; the three-state enum wins.)
#[derive(Debug, PartialEq)]
pub enum HostProbe {
    Live { proto: u32, session: String, panes: usize, attached_gui: bool },
    NoSocket,
    Unresponsive,
}
/// Tiny sync client: connect + hello{kind:Cli} + one reply, bounded by
/// `budget`. Used by resolve_session_hosted's new tier (instance.rs:299) and
/// by a losing `serve` racer deciding whether to exit 0.
pub fn probe_host(key: &str, budget: Duration) -> HostProbe;
```

### app/src/serve.rs (new) — the verb's permanent entry

```rust
/// `terminal-delight serve --session <id>` — same shape as ctl::run_cli
/// (ctl.rs:873). Slice 0 body: parse args, print "the session host lands in a
/// later slice", exit 2 — the verb is RESERVED and can never boot a GUI, while
/// a typo'd `sevre` is refused by the allowlist. Host slice body: exit(
/// host::run_serve(args)). (Resolution: one draft dispatched `serve` inline in
/// main; the serve.rs entry wins so slice 0 ships a real dispatch target.)
pub fn run_cli(args: &[String]) -> i32;

/// `--session` is required — a host must never default to "whatever session is
/// newest" (the #311/#299/#272/#151 cross-wiring class).
pub struct ServeArgs { pub session: String }
pub fn parse_args(args: &[String]) -> Result<ServeArgs, String>;
```

### app/src/main.rs (modified) — dispatch, caps, SavedNode, boot, attach, save routing

```rust
// ── Slice 0: the dispatch allowlist (replaces main.rs:18337-18398) ──

/// Everything `main` may do with argv[1], decided by ONE pure function so the
/// whole contract is unit-testable without gpui. Kills #314: today an unknown
/// positional falls through flag_reply (main.rs:18320) into the GUI and
/// mutates on-disk session state. (Resolution: one draft proposed a smaller
/// `positional_reply`; the typed Launch/Verb enum with injected `is_dir` wins
/// — same contract, fully testable.)
#[derive(Debug, PartialEq)]
enum Launch {
    Verb(Verb),                                   // headless: handler gets argv[2..], no gpui
    /// Print and exit: code 0 → stdout, else stderr. Flags via flag_reply
    /// (main.rs:18320, unchanged); NEW arm: unknown positional → code 2 + USAGE.
    Reply { text: String, code: i32 },
    /// `open_here` is Some only for a positional naming an EXISTING directory —
    /// the reserved open-here slot, validated now instead of left as the #314
    /// hole. Threaded as the fresh-session first-pane cwd; a restore ignores it.
    Window { open_here: Option<std::path::PathBuf> },
}
#[derive(Debug, PartialEq, Clone, Copy)]
enum Verb {
    EmitDemo,     // "--td-emit-demo" → demo::emit_and_block()  demo.rs:27
    Ctl,          // → ctl::run_cli          ctl.rs:873
    Mcp,          // → ctl::run_mcp_cli      ctl.rs:1095
    AgentUsage,   // → usage::run_cli        usage.rs:428
    AgentVitals,  // → vitals::run_cli       vitals.rs:1729
    Probe,        // → probe_cli             main.rs:18260
    Serve,        // → serve::run_cli        serve.rs (new)
}
/// Order: known verbs → '-'-leading via flag_reply → existing-dir positional →
/// refusal. `is_dir` injected so tests exercise the directory arm without a
/// filesystem (the scratch_decision test style, main.rs:17095+).
fn dispatch(first: Option<&str>, is_dir: impl Fn(&str) -> bool) -> Launch;
// The pinning test subcommands_and_bare_arguments_still_reach_the_window
// (main.rs:17173) is REWRITTEN in the same commit: existing dir reaches the
// window; typo'd verbs are refused.

// ── Caps (decision 5, amended) ──

/// New splits stop at 4. Enforced ONLY at split() (main.rs:4682) and new-pane
/// creation — never by the loader.
const MAX_PANES: usize = 4;                       // was 8, main.rs:75
/// What a LOADED tab may still hold: legacy files saved up to 8 panes and the
/// CRT warp shader renders 8 tubes. build_node (main.rs:2406), plan_attach and
/// orphan adoption tolerate up to this; MAX_TAB_BADGES assert (main.rs:18102)
/// and badge_overflow test (main.rs:16501) re-point here.
const LEGACY_PANE_CEILING: usize = 8;
const _: () = assert!(MAX_PANES <= LEGACY_PANE_CEILING);

// ── SavedNode (main.rs:431) ──

enum SavedNode {
    Leaf {
        // …existing six fields unchanged (appearance/cwd/resume/name/logo/note)…
        /// The host pane this leaf shows. None = written pre-split, or by a
        /// serverless window — unknown is not zero; never Some(0) by default.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pane_id: Option<u64>,
    },
    Split { /* unchanged */ },
}

// ── Boot (main.rs:18333, after the scratch decision) ──

enum Boot {
    Scratch { seed: Option<session::PaneRestore> },  // TD_SCRATCH/TD_SEED_*/TD_DEMO_STATE, unchanged
    Legacy  { key: String, claim: instance::Claim }, // today's resolve_session (instance.rs:299)
    Hosted  { handle: hostctl::HostHandle },         // TD_SESSIOND=1: resolve_session_hosted + spawn/attach
}
fn resolve_boot() -> Boot;

// ── Workspace attach (main.rs:1975 / :2481) ──

/// Everything Workspace needs while attached. gpui main thread only → Rc<RefCell>.
struct AttachCtx {
    handle: std::rc::Rc<std::cell::RefCell<hostctl::HostHandle>>,
    panes: Vec<hostproto::PaneInfo>,             // list-panes at boot; refreshed on host events
    /// Set on HostEvent::Lost: panes freeze, save() goes inert — a stolen
    /// window must never write the TOML the host owns.
    lost: std::cell::Cell<bool>,
}

/// PURE attach planning — the testable core of restore-under-a-host.
enum LeafPlan {
    Bind    { pane_id: u64 },                            // pane_id live on the host
    Respawn { restore: session::PaneRestore },           // absent/dead → today's recipe via spawn-pane
}
struct AttachPlan {
    tabs: Vec<Vec<LeafPlan>>,                            // per leaf, in tab tree order
    /// Live host panes the layout did not claim — one new tab each. THE pinned
    /// invariant: the host's live pane table beats the TOML; a checkpoint-stale
    /// layout can never lose a running pane. A pane_id claimed twice binds the
    /// first leaf, respawns the second.
    orphans: Vec<hostproto::PaneInfo>,
}
fn plan_attach(saved: &[SavedTab], live: &[hostproto::PaneInfo]) -> AttachPlan;

impl Workspace {
    /// build (main.rs:2481) gains the attach ctx; None = scratch/demo/legacy,
    /// which run today's branches byte-for-byte.
    fn build(scratch: bool, demo: bool, seed: Option<session::PaneRestore>,
             attach: Option<AttachCtx>, window: &mut Window, cx: &mut Context<Self>) -> Self;
    /// The hosted restore: plan_attach over load_state() + list-panes, per leaf
    /// make_pane_attached / make_pane_host_spawned, orphans as new tabs, then
    /// publish the ctl session alias. Serverless build_node (main.rs:2406) untouched.
    fn build_attached(&mut self, saved: StateFile, window: &mut Window, cx: &mut Context<Self>);
    /// save (main.rs:3015) routes: attached → HostHandle::save (host is the
    /// single TOML writer); attached-but-lost → inert; else → the
    /// persist_primary_state wrapper as today. scratch/released() guards
    /// (main.rs:3017-3025) unchanged.
    fn save(&self, cx: &App);
}

/// Attach one live host pane as a TerminalView: open_pane_stream →
/// term::attach_in → the same subscription wiring as make_pane_restored
/// (main.rs:2270); saved appearance/name/logo/note re-applied as build_node does.
fn make_pane_attached(pane: &hostproto::PaneInfo, saved: Option<&SavedNode>,
    attach: &AttachCtx, window: &mut Window, cx: &mut Context<Workspace>) -> Entity<TerminalView>;
/// Respawn-under-a-host: HostHandle::spawn_pane(recipe) then make_pane_attached.
fn make_pane_host_spawned(restore: session::PaneRestore, attach: &AttachCtx,
    window: &mut Window, cx: &mut Context<Workspace>) -> Entity<TerminalView>;

/// persist_primary_state (main.rs:1250) becomes a one-line wrapper over
/// session::persist_primary — kept so its ~6 call sites and the richness-guard
/// tests do not churn.
fn persist_primary_state(body: &str, new_leaves: usize, new_tabs: usize, allow_shrink: bool);
```

### app/src/session.rs (modified) — the relocated single write chokepoint

```rust
/// Shallow counts from an existing session TOML WITHOUT deserialising the
/// layout envelope (StateFile.panes is a top-level int, main.rs:1067, read the
/// way instance::toml_top_level_usize does at instance.rs:420). None =
/// pre-field/unreadable file — unknown, never zero.
pub struct SavedCounts { pub leaves: Option<usize>, pub tabs: Option<usize> }
pub fn saved_counts(path: &Path) -> SavedCounts;

/// The one persist-verdict type, on the wire and off it (serde-derived; this
/// module is gpui-free). Io failures are io::Result/Outcome::Err, not a variant.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum PersistOutcome {
    Written,
    RefusedShrink { old_leaves: usize, new_leaves: usize, old_tabs: usize, new_tabs: usize },
}

/// RELOCATED VERBATIM from main.rs:1250 (with is_catastrophic_shrink :1189 and
/// rotate_state_backup :1207): shrink guard, .last-good snapshot, 10 rotated
/// backups, then write_atomic (session.rs:74). The host calls this; the legacy
/// GUI calls it via the main.rs wrapper. Now RETURNS its outcome instead of
/// only eprintln-ing, so the host's Saved reply is truthful.
pub fn persist_primary(path: &Path, body: &str, new_leaves: usize, new_tabs: usize,
                       allow_shrink: bool) -> io::Result<PersistOutcome>;
```

### app/src/term.rs (modified) — the seam

Ground truth verified in the vendored crate
(`~/.cargo/registry/src/…/alacritty_terminal-0.26.0/src/`):
`EventedReadWrite` tty/mod.rs:65-78, `EventedPty` tty/mod.rs:92-97
(`ChildEvent::Exited(Option<ExitStatus>)` :82-85), `EventLoop::new` bounds
event_loop.rs:57-69 (`T: tty::EventedPty + event::OnResize + Send + 'static`),
`FairMutex` sync.rs:28 (`lease` = next-slot only) / :33-38 (`lock` = next then
data) / :41-43 (`lock_unfair` = data only), `pty_read` holds the lease for the
whole cycle (event_loop.rs:117), reads at :122 before the data lock :140, every
loop exit leaves `unprocessed == 0`; `Options.env` tty/mod.rs:36; `Pty` impls
unix.rs:323-406.

```rust
/// MODIFIED (tuple → named struct + one flag). A replica must SWALLOW
/// Event::PtyWrite: the HOST answers DA/DSR (the term.rs:7-9 contract — vim
/// querying while detached still needs replies) and pane.rs:2963 bounces
/// unconditionally, so an unfiltered replica would double every query reply.
/// Generation still bumps on every event, suppressed or not (term.rs:45-49).
/// Only construction site is term.rs:113 (grep-verified).
#[derive(Clone)]
pub struct EventProxy {
    tx: UnboundedSender<TermEvent>,
    generation: Arc<AtomicU64>,
    suppress_pty_write: bool,      // true only on replicas built by attach_in
}
impl EventListener for EventProxy { fn send_event(&self, event: TermEvent); }

/// Everything tty::new needs, plus the env the host stamps.
pub struct SpawnSpec {
    pub size: GridSize,                    // term.rs:28
    pub cell_width: u16,
    pub cell_height: u16,
    pub cwd: Option<std::path::PathBuf>,   // vanished dir falls back, as today (term.rs:123)
    pub env: Vec<(String, String)>,        // -> tty::Options.env (vendored tty/mod.rs:36)
}

/// The tty::new half of today's spawn_in body (term.rs:115-141; vendored
/// tty::new unix.rs:195). TD_DEMO handling stays inside (term.rs:131-138).
pub fn spawn_pty(spec: &SpawnSpec) -> io::Result<alacritty_terminal::tty::Pty>;

/// The Term+EventLoop half (term.rs:142-158), generic so host and client pass
/// TeePty / SocketPty. Bounds are exactly EventLoop's own. master/shell_pid are
/// passed in because a generic T has no .file()/.child() accessors (Pty's,
/// unix.rs:110/:114). `proxy` lets attach_in set suppress_pty_write.
pub fn wire_event_loop<T>(pty: T, size: GridSize, master: Option<std::fs::File>,
                          shell_pid: u32, proxy: EventProxy) -> io::Result<Session>
where T: alacritty_terminal::tty::EventedPty + alacritty_terminal::event::OnResize + Send + 'static;

// spawn_in (term.rs:105) becomes a 3-line composition of the two, signature
// unchanged, env=[] — scratch/demo/serverless keep it forever.

/// spawn_in's hosted twin: spawn_pty + TeePty wrap + wire_event_loop, so live
/// bytes fan out to the pane's sink registry. env carries TD_SESSION/TD_PANE_ID.
pub fn spawn_hosted(spec: &SpawnSpec) -> io::Result<(Session, crate::gridwire::TeeSinks)>;

/// What the GUI hands attach_in after the control dance (hello → attach-pane
/// ack → byte-stream conn with its `stream <pane_id>` line sent; the next bytes
/// readable are the gridwire snapshot, then live tee'd PTY bytes).
pub struct AttachStreams {
    pub bytes: std::os::unix::net::UnixStream,
    /// Resize is a control verb, never a byte-stream fact. Called from
    /// SocketPty::on_resize on the EventLoop thread → Send + 'static.
    pub send_resize: Box<dyn FnMut(WindowSize) + Send + 'static>,
}

/// THE SEAM — beside spawn_in. Stock EventLoop over SocketPty into a replica
/// Term. Returns the SAME Session type (term.rs:60), so all 33 pane.rs
/// term.lock() sites (styled_lines :5027, selection :4238, scroll :4159,
/// mirror_document :2312) run against the replica unchanged; selection/scroll
/// stay client state and the generation counter stays the invalidation token BY
/// CONSTRUCTION. master: None (term.rs:67 is already Option — tcgetpgrp is
/// meaningless client-side; mode arrives as Push::Mode). shell_pid is the
/// host-reported attribute. (Resolution: one draft asked for a bare
/// `-> io::Result<Session>`; the (Session, ReplicaGuard) pair from the owning
/// domain wins — the guard is where divergence checking lives.)
pub fn attach_in(size: GridSize, cell_width: u16, cell_height: u16,
                 streams: AttachStreams, shell_pid: u32)
                 -> io::Result<(Session, crate::gridwire::ReplicaGuard)>;

/// Client-side adapter: alacritty's public PTY traits over a unix stream.
pub struct SocketPty {
    stream: std::os::unix::net::UnixStream,     // nonblocking; the fd the Poller watches (key 0)
    reader: CountingReader,                     // over a try_clone of `stream`
    writer: std::os::unix::net::UnixStream,     // try_clone of `stream`
    /// Self-pipe mirroring tty::Pty::signals (unix.rs:105-106). On socket EOF
    /// the reader writes one byte here (key 1), waking the run loop's
    /// child-event arm (event_loop.rs:256-271) which exits cleanly — Level-mode
    /// polling would otherwise spin on a forever-readable closed fd.
    eof_pipe: (std::os::unix::net::UnixStream, std::os::unix::net::UnixStream),
    send_resize: Box<dyn FnMut(WindowSize) + Send + 'static>,
}
/// Counts every byte handed to the replica parser — the divergence guard's
/// client-side clock (shared Arc with ReplicaGuard.read_offset).
pub struct CountingReader {
    inner: std::os::unix::net::UnixStream,
    offset: Arc<AtomicU64>,
    eof_tx: std::os::unix::net::UnixStream,
    saw_eof: bool,
}
impl io::Read for CountingReader { fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>; }
impl tty::EventedReadWrite for SocketPty {                       // tty/mod.rs:65
    type Reader = CountingReader;
    type Writer = std::os::unix::net::UnixStream;
    unsafe fn register(&mut self, poll: &Arc<polling::Poller>,
        interest: polling::Event, mode: polling::PollMode) -> io::Result<()>;  // both fds, like Pty unix.rs:328-346
    fn reregister(&mut self, poll: &Arc<polling::Poller>,
        interest: polling::Event, mode: polling::PollMode) -> io::Result<()>;
    fn deregister(&mut self, poll: &Arc<polling::Poller>) -> io::Result<()>;
    fn reader(&mut self) -> &mut CountingReader;
    fn writer(&mut self) -> &mut std::os::unix::net::UnixStream;
}
impl tty::EventedPty for SocketPty {                             // tty/mod.rs:92
    /// None until EOF; then Some(ChildEvent::Exited(None)) — exit status
    /// genuinely unknown here, and ChildEvent models that (Option, :84).
    /// Authoritative pane exit arrives as Push::Exit.
    fn next_child_event(&mut self) -> Option<tty::ChildEvent>;
}
impl event::OnResize for SocketPty {                             // event.rs:98
    fn on_resize(&mut self, ws: WindowSize);                     // → (self.send_resize)(ws)
}

/// Host-side wrapper: everything delegates to the real tty::Pty (unix.rs:102)
/// except Reader. (Resolution: one draft held a single Option<TeeSink> in
/// host.rs; the gridwire::TeeSinks Vec registry wins — "nothing assumes exactly
/// one attached client" is a binding decision, and steal becomes registry
/// policy rather than type shape.)
pub struct TeePty { inner: alacritty_terminal::tty::Pty, reader: TeeReader }
pub struct TeeReader {
    /// try_clone of inner.file() — same open file description; the Poller
    /// watches the ORIGINAL fd (register delegates to inner). Same clone trick
    /// term.rs:140 already uses for the mode-watcher master handle.
    master: std::fs::File,
    sinks: crate::gridwire::TeeSinks,
}
impl io::Read for TeeReader {
    /// read(master) → fan out via try_send to every live sink → return. Runs
    /// inside pty_read's lease (event_loop.rs:117→:122): MUST NOT block on a
    /// slow client — a full sink is marked dead (that client re-attaches for a
    /// fresh fenced snapshot); the pane never stalls.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
}
impl tty::EventedReadWrite for TeePty {
    type Reader = TeeReader;
    type Writer = std::fs::File;                 // = Pty's Writer, unix.rs:325
    unsafe fn register(&mut self, poll: &Arc<polling::Poller>,
        interest: polling::Event, mode: polling::PollMode) -> io::Result<()>;  // → inner
    fn reregister(&mut self, poll: &Arc<polling::Poller>,
        interest: polling::Event, mode: polling::PollMode) -> io::Result<()>;  // → inner
    fn deregister(&mut self, poll: &Arc<polling::Poller>) -> io::Result<()>;   // → inner
    fn reader(&mut self) -> &mut TeeReader;      // the one non-delegating method
    fn writer(&mut self) -> &mut std::fs::File;  // → inner.writer(), unix.rs:377
}
impl tty::EventedPty for TeePty { fn next_child_event(&mut self) -> Option<tty::ChildEvent>; } // → inner :382
impl event::OnResize for TeePty { fn on_resize(&mut self, ws: WindowSize); }                   // → inner (TIOCSWINSZ) :406
```

### app/src/gridwire.rs (new) — snapshot encoder, tee registry, THE LEASE FENCE, divergence guard

Module doc carries THE LEASE INVARIANT (the upgrade tripwire, stated per the
panel's correction — the *lease* argument, not the lock argument): `pty_read`
holds `FairMutex::lease()` (next-slot only, sync.rs:28-30) for its whole cycle
(event_loop.rs:117), reads PTY bytes at :122 before the data lock (:140), and
every exit of its loop leaves `unprocessed == 0` — so when the lease releases,
every byte read has been parsed into the Term. A fence taking `lease()` then
`lock_unfair()` therefore (a) cannot interleave inside a read cycle, (b)
observes a Term containing exactly the bytes tee'd so far, (c) cannot deadlock
— identical acquisition order to pty_read. NEVER `lock()` while holding
`lease()` (lock re-takes next, sync.rs:36; parking_lot is not reentrant).
Re-verify all four line-cites on any alacritty bump. One documented bend:
pty_read's hard-error exit (event_loop.rs:133) can leave read-but-unparsed
bytes that WERE tee'd — the pane is dying and no GridCheck fires post-exit.

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SinkId(u64);

/// Per-pane registry shared between TeeReader (EventLoop thread) and the fence
/// (host threads). Plain std Mutex held only for push/register/remove — never
/// across a parse. Vec of sinks: nothing here assumes exactly one attached
/// client; steal is the host's policy (remove then attach).
#[derive(Clone)]
pub struct TeeSinks(Arc<std::sync::Mutex<TeeState>>);
struct TeeState { sinks: Vec<TeeSink>, next_id: u64 }
struct TeeSink {
    id: SinkId,
    tx: std::sync::mpsc::SyncSender<Arc<[u8]>>,  // bounded; try_send only — overflow ⇒ dead, pane unharmed
    /// Cumulative bytes ENQUEUED since attach — snapshot, re-snapshot and PTY
    /// bytes alike. The socket is FIFO, so this counter and the client's
    /// CountingReader tick the same clock; GridCheck.stream_offset quotes it.
    enqueued: u64,
    dead: bool,
}
impl TeeSinks {
    pub fn new() -> Self;
    pub fn remove(&self, id: SinkId) -> bool;    // steal / detach path
    pub fn is_dead(&self, id: SinkId) -> bool;   // overflow forces re-attach
}

/// What the host gets back from a fenced attach; it drains `chunks` to the
/// byte-stream socket. Snapshot ordering is INHERENT: the snapshot is enqueued
/// into the same queue before the sink goes live.
pub struct AttachReceipt {
    pub sink: SinkId,
    pub stream_offset: u64,                      // per-sink enqueued total right after the snapshot
    pub cols: u16,
    pub rows: u16,
    pub chunks: std::sync::mpsc::Receiver<Arc<[u8]>>,
}

/// THE FENCE. Called AFTER the client's size is applied (resize-then-snapshot,
/// so snapshot dims == replica dims; Session::resize, term.rs:85). Fixed order:
/// 1 lease() · 2 lock_unfair() · 3 enqueue encode_snapshot(&mut t) into the new
/// sink · 4 register the sink live · 5 guards drop ⇒ every PTY byte is in the
/// snapshot XOR tee'd to this sink — never neither, never both.
pub fn fenced_attach<T: EventListener>(term: &FairMutex<Term<T>>, tee: &TeeSinks,
                                       sink_capacity: usize) -> AttachReceipt;

/// Host half of the divergence guard: same fence, hash instead of encode.
/// Called at generation quiescence (host gen stable + sink queue drained);
/// result pushed as Push::GridCheck. None if the sink is dead.
pub fn fenced_hash<T: EventListener>(term: &FairMutex<Term<T>>, tee: &TeeSinks,
                                     sink: SinkId) -> Option<GridCheck>;

/// The LOUD repair: under the same fence, enqueue RIS (ESC c) + a fresh
/// snapshot into the surviving sink. In-band: the replica's own parser resets
/// and replays, so Session is never rebuilt and pane.rs stays untouched.
/// Returns the sink's enqueued total at injection. Err if the sink is dead.
pub fn fenced_resnapshot<T: EventListener>(term: &FairMutex<Term<T>>, tee: &TeeSinks,
                                           sink: SinkId) -> io::Result<u64>;

/// One integrity probe, host → client, rides Push::GridCheck.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct GridCheck {
    pub pane: crate::hostproto::PaneId,
    pub stream_offset: u64,   // per-sink enqueued bytes at the fence
    pub hash: u64,
}

/// Snapshot encoder: authoritative Term → VT bytes reproducing scrollback +
/// screen + cursor + modes in a fresh same-sized Term. READ-ONLY — &Term, by
/// design: the encoder must never mutate the authoritative terminal.
/// (Resolution history: one draft reached the inactive primary grid via a
/// temporary double Term::swap_alt and claimed the flip pair neutral; the
/// existence verifier falsified that against the vendored source — the mode
/// bit toggles UNCONDITIONALLY at term/mod.rs:732, so the second flip runs
/// the destructive branch (:715-724) and its reset_region ERASES the live alt
/// grid. One encode would have wiped a running vim. No public accessor
/// reaches the inactive grid (term/mod.rs:287, private field), so the alt leg
/// is read-only-degraded and healed on alt exit — see below.)
/// Emission: primary history+screen rows Line(-history_size())..bottommost_line()
/// (grid/mod.rs:504-517; cells via grid[Line][Column] as term.rs:224), SGR runs
/// from Cell (cell.rs:134-197), CR/LF only at non-WRAPLINE row ends so wrap
/// flags re-derive on replay; cursor via renderable_content().cursor
/// (term/mod.rs:637) + DECSCUSR (:942); TermMode-derived modes (term/mod.rs:55)
/// and charset state (grid/mod.rs:42).
/// ALT-SCREEN pane (vim, htop): the active alt grid is the only one readable,
/// so the encoder emits CSI ?1049h + the alt paint + alt cursor + modes ONLY —
/// the replica's primary grid and scrollback start empty behind it. The heal:
/// watcher_loop already tracks TermMode; on the alt→primary transition it
/// pushes a fenced re-snapshot for that pane (same splice path as attach),
/// which now covers full history — and the divergence guard stays the backstop
/// if the transition is ever missed. Nothing is ever lost host-side; the
/// replica is behind only while the pane stays in alt.
pub fn encode_snapshot<T: EventListener>(term: &Term<T>) -> Vec<u8>;

/// Deterministic cross-process content hash: FNV-1a 64 over a canonical
/// serialization (no HashMap iteration, no per-process seeds). COVERS every
/// history+screen row's cells, cursor point, ALT_SCREEN + input-relevant
/// TermMode bits. EXCLUDES display_offset and selection — client state by
/// construction; the replica may be scrolled or selecting while converged.
pub fn grid_hash<T: EventListener>(term: &Term<T>) -> u64;

/// Client half of the guard, returned by term::attach_in beside Session
/// (Session grows no fields; pane.rs never sees this type).
pub struct ReplicaGuard {
    term: Arc<FairMutex<Term<crate::term::EventProxy>>>,  // clone of Session.term       term.rs:61
    generation: Arc<AtomicU64>,                           // clone of Session.generation term.rs:73
    read_offset: Arc<AtomicU64>,                          // shared with CountingReader
}
pub enum GuardVerdict {
    /// Replica hasn't consumed up to check.stream_offset yet, or its generation
    /// is still moving — re-poll later. NOT a mismatch: unknown is not zero.
    NotYet,
    Match,
    /// The loud path: caller logs both hashes and sends Request::Resnapshot.
    Mismatch { host: u64, replica: u64 },
}
impl ReplicaGuard {
    /// Takes the replica FairMutex's OWN lease→lock_unfair fence (same
    /// invariant, client side — the replica EventLoop is stock too).
    pub fn check(&self, check: &GridCheck) -> GuardVerdict;
    pub fn read_offset(&self) -> u64;
}
```

### app/src/host.rs (new) — the session host

```rust
/// `terminal-delight serve --session <id>` body (called by serve::run_cli in
/// the host slice). gpui is never initialized on this path. Order:
/// tty::setup_env() → instance::claim_in(&instance::config_dir(), key)
/// (instance.rs:471, fail-closed verbatim; not owned → if probe_host answers
/// Live, exit 0 as the arbitration loser, else exit 1) → instance::bind
/// (instance.rs:86) → bind session-<key>.sock → loops. The SPAWNER setsids
/// (Command::pre_exec(libc::setsid)); serve itself never forks.
pub fn run_serve(args: ServeArgs) -> i32;

> **REVERSED 2026-09-10 — the `gui_conn` slot and the steal it serves are not
> built and will not be.** Everything from `gui_sink` and `gui_conn` below
> through `handle_request`'s "a Gui-kind hello drops the previous gui_conn
> (steal, decision 1)" describes a policy Parker deleted on the reconciled
> fresh-agent review; the reasoning is recorded under "Amendment, 2026-09-10"
> in `02-architecture.md`. It is left here rather than edited out because this
> page is the record of what was designed, and because the two-line sketch of
> the steal is precisely what nobody built for the life of the branch.
>
> What is true instead: no window slot, no window-level supersede, and
> `ClientKind` gates nothing. Pane-level supersede — `TeeSinks::remove` on the
> old sink at stream-connect, the `serial` on an attachment — is real and is
> what `splice_attach` below still describes. Two windows share a session's
> panes, most recent attach winning per pane (issue 351).

/// One live pane: the authoritative side. `session` is a stock term::Session
/// (term.rs:60) built by term::spawn_hosted over a TeePty.
pub(crate) struct HostPane {
    id: hostproto::PaneId,
    session: crate::term::Session,
    sinks: crate::gridwire::TeeSinks,            // per-pane registry, shared with its TeeReader
    gui_sink: Mutex<Option<crate::gridwire::SinkId>>, // v1 policy: one GUI sink; steal replaces it
    geom: Mutex<hostproto::PaneGeom>,
    mode: Mutex<hostproto::WireMode>,
    exited: AtomicBool,
}

/// The host: owns the pane table, mints durable ids, holds the flock for life.
pub(crate) struct Host {
    key: String,
    panes: Mutex<BTreeMap<hostproto::PaneId, Arc<HostPane>>>,
    next_pane: AtomicU64,                        // monotonic per session, never reused
    ever_spawned: AtomicBool,                    // exit-on-last-close arms only after first spawn
    layout: Mutex<Option<(u32, toml::Value)>>,   // (schema, opaque client envelope), disk-seeded at boot
    pushers: Mutex<Vec<PushHandle>>,             // every control conn; N clients is normal
    gui_conn: Mutex<Option<PushHandle>>,         // the current Gui-kind hello; a new one steals it
}

/// Generous sanity cap only — the real per-window cap is the client's
/// MAX_PANES=4. Must exceed LEGACY_PANE_CEILING=8 so a legacy restore never
/// kills a running pane (binding decision 5).
pub(crate) const HOST_PANE_SANITY_CAP: usize = 64;

impl Host {
    fn accept_loop(self: &Arc<Self>, listener: UnixListener) -> !;
    /// Auth is this check and nothing else; no protocol field carries identity.
    /// getsockopt(SOL_SOCKET, SO_PEERCRED) → libc::ucred; uid != geteuid() →
    /// drop before any read.
    fn peer_uid(stream: &UnixStream) -> io::Result<u32>;
    /// First line discriminates: '{' → NDJSON control conn; "stream <pane_id>"
    /// → byte stream.
    fn route_conn(self: &Arc<Self>, stream: UnixStream);
    fn control_loop(self: &Arc<Self>, stream: UnixStream);
    /// hello handles hostproto::version_check first; a Gui-kind hello drops the
    /// previous gui_conn (steal, decision 1) — never a refusal.
    fn handle_request(self: &Arc<Self>, req: hostproto::Request, conn: &PushHandle) -> hostproto::Reply;

    /// term::spawn_hosted with env [(TD_SESSION,key),(TD_PANE_ID,id)]; if
    /// `resume` is Some, the host types the resume line (the pane.rs:2480-2482
    /// recipe, executed host-side).
    fn spawn_pane(&self, cwd: Option<String>, resume: Option<String>, geom: hostproto::PaneGeom)
        -> hostproto::Outcome<hostproto::PaneInfo>;
    /// Control half of attach: Session::resize first (term.rs:85 — resize-then-
    /// snapshot so snapshot dims == replica dims), record intent, reply.
    fn attach_pane(&self, pane: hostproto::PaneId, geom: hostproto::PaneGeom)
        -> hostproto::Outcome<hostproto::PaneInfo>;
    /// THE atomic splice, run on the byte-stream connection after its
    /// "stream <id>" line. Steal first: TeeSinks::remove(old gui_sink) +
    /// shutdown(Both) on the old stream + Push::Detached{Superseded}; then
    /// gridwire::fenced_attach (lease → lock_unfair → snapshot → register —
    /// pty_read's own acquisition order, see the gridwire module doc), then a
    /// writer thread drains AttachReceipt.chunks to the stream and input_pump
    /// starts. (Resolution: one draft fenced at the attach-pane verb, one at
    /// stream connect; stream connect wins — the socket the snapshot is written
    /// to must exist when the fence closes.)
    fn splice_attach(&self, pane: &Arc<HostPane>, stream: UnixStream) -> io::Result<()>;

    /// close = intent (the approved split): SIGHUP the child tree
    /// (libc::kill(-(shell_pid as i32), SIGHUP)), Msg::Shutdown to the
    /// EventLoop, remove from the table, drop Session — the master-close HUP is
    /// today's drop-is-close (main.rs:77-82) kept.
    fn close_pane(&self, pane: hostproto::PaneId) -> hostproto::Outcome<hostproto::CloseInfo>;

    fn save(&self, schema: u32, body: String, allow_shrink: bool)
        -> hostproto::Outcome<crate::session::PersistOutcome>;
    /// The 30s checkpoint body, also the Save handler's tail: merge live capture
    /// into the envelope, recount leaves/tabs from the walk, then
    /// session::persist_primary (→ shrink guard, rotate_state_backup,
    /// write_atomic session.rs:74). Host = the single TOML writer.
    fn persist_now(&self, allow_shrink: bool) -> io::Result<crate::session::PersistOutcome>;

    fn broadcast(&self, push: &hostproto::Push);
    fn watcher_loop(self: Arc<Self>);      // 800ms attached, backs off to 5s with no client
                                           // (Gate 2 graft: detached hosts must not become
                                           // the new battery drain); classify + sticky-alt
                                           // rule host-side; also watches TermMode for the
                                           // alt→primary transition → fenced re-snapshot push
    fn checkpoint_loop(self: Arc<Self>);   // 30s attached, 5min detached → persist_now(false);
                                           // runs with zero clients; attach restores cadence
    /// Quiescence scheduler for the divergence guard: per attached pane, when
    /// generation stable ~500ms and the sink queue is drained →
    /// gridwire::fenced_hash → Push::GridCheck; also emits
    /// Push::Detached{SlowConsumer} when TeeSinks::is_dead flips.
    fn guard_loop(self: Arc<Self>);
    /// Consumes Session.events (term.rs:64). MUST bounce Event::PtyWrite into
    /// the notifier (the term.rs:7-9 contract — the HOST owns the bounce; the
    /// replica suppresses its own, so DA/DSR is answered exactly once).
    /// Event::ChildExit(status) → Push::Exit + mark exited + remove pane.
    fn drain_events(self: Arc<Self>, pane: Arc<HostPane>, rx: UnboundedReceiver<TermEvent>);
    /// client bytes → Notifier::notify (vendored event_loop.rs:335) → Msg::Input → PTY.
    fn input_pump(pane: Arc<HostPane>, stream: UnixStream);
    /// persist_now, Msg::Shutdown each EventLoop, instance::release()
    /// (instance.rs:98 — disarm-before-unlock ordering kept verbatim), unlink
    /// socket, exit.
    fn shutdown(&self, code: i32) -> !;
}

/// Non-destructive merge over toml::Value (NEVER a StateFile round-trip — a
/// newer client's unknown #319 fields must survive the host untouched): walk
/// tabs[*].node recursively, update cwd/resume only on Leaf tables carrying
/// pane_id, from session::capture (session.rs:59) per live pane. Leaf counts
/// for the shrink guard come from the same walk.
fn merge_capture_into_layout(layout: &toml::Value,
    live: &BTreeMap<hostproto::PaneId, crate::session::PaneRuntime>) -> toml::Value;

/// Relocated classification: tcgetpgrp + /proc comm/cmdline, logic from pane.rs
/// foreground_mode (:203-217) + PaneMode::classify (:41-63), returning the wire
/// enum; sticky-alt rule (pane.rs:2516-2526) applied by watcher_loop.
fn classify_foreground(master: &std::fs::File, shell_pid: u32) -> hostproto::WireMode;
```

### app/src/hostctl.rs (new) — GUI-side client

```rust
/// Events the Workspace pumps into panes. Wire pushes pass through untranslated
/// — one spelling of mode/exit/detach lives in hostproto. Lost is local: control
/// connection EOF (host died, or this GUI was stolen).
pub enum HostEvent { Push(hostproto::Push), Lost }

/// An attached control connection (post-hello, kind: Gui — the host DROPS any
/// previous GUI connection on this hello: second attach steals, decision 1).
pub struct HostHandle { /* BufReader<UnixStream>, session: String, proto: u32 */ }

impl HostHandle {
    /// connect + Hello{PROTO_VERSION, Gui, build}; Reply::VersionMismatch here
    /// drives the shutdown-then-TOML-recovery degrade (see host-crash stack).
    pub fn attach_gui(id: &str) -> io::Result<HostHandle>;
    pub fn session(&self) -> &str;
    pub fn list_panes(&mut self) -> io::Result<Vec<hostproto::PaneInfo>>;
    /// Second connection to the same socket, first line `stream <pane_id>`;
    /// handed verbatim into term::AttachStreams.bytes.
    pub fn open_pane_stream(&mut self, pane: hostproto::PaneId) -> io::Result<UnixStream>;
    pub fn spawn_pane(&mut self, cwd: Option<&str>, resume: Option<&str>,
                      geom: hostproto::PaneGeom) -> io::Result<hostproto::PaneInfo>;
    pub fn close_pane(&mut self, pane: hostproto::PaneId) -> io::Result<hostproto::CloseInfo>;
    pub fn resize(&mut self, pane: hostproto::PaneId, geom: hostproto::PaneGeom) -> io::Result<()>;
    pub fn resnapshot(&mut self, pane: hostproto::PaneId) -> io::Result<u64>;
    /// The persistence redirect. RefusedShrink comes back, never a silent ack.
    pub fn save(&mut self, schema: u32, body: &str, allow_shrink: bool)
        -> io::Result<crate::session::PersistOutcome>;
    /// Split the connection: a blocking reader thread feeding HostEvents.
    pub fn take_events(&mut self) -> futures::channel::mpsc::UnboundedReceiver<HostEvent>;
}

/// Spawn `terminal-delight serve --session <id>` setsid-detached (current_exe,
/// like spawn_seeded_window main.rs:18236), then wait bounded for hello. The
/// HOST claims the flock (instance.rs:471); a loser of the race exits 0 and
/// this fn re-probes and attaches to the winner — two $TD_SESSION launches
/// contend on the same kernel primitive as today.
pub fn spawn_host(id: &str, budget: Duration) -> io::Result<HostHandle>;
```

### app/src/instance.rs (modified)

```rust
/// How the resolved session is reached. Legacy keeps today's Claim (instance.rs:459).
pub enum Route {
    AttachLive,          // a live host answered; GUI holds NO flock
    SpawnHost,           // no live host: spawn `serve --session <id>`, then attach
    Legacy(Claim),       // TD_SESSIOND unset — today's claim-and-own path, byte-for-byte
}
pub struct Resolved { pub id: String, pub route: Route }

/// TD_SESSIOND=1 resolution. Tier order (panel fork resolution):
///  1. $TD_SESSION (explicit_session, instance.rs:232) — probe: Live→AttachLive
///     (attach steals), else SpawnHost.
///  2. FIRST-RANKED ADOPTION TIER: any session whose host answers hello with
///     attached_gui:false — ordered by rank() (instance.rs:378) → AttachLive.
///     This IS the kill-relaunch case.
///  3. most-recently-saved TOML with no live host → SpawnHost (the host claims;
///     a legacy GUI holding the flock fails the spawn → next candidate).
///  4. fresh id (fresh_session, instance.rs:393 shape) → SpawnHost.
pub fn resolve_session_hosted() -> Resolved;

/// Injectable core, like resolve_session_in (instance.rs:309): probe stands in
/// for hostproto::probe_host so every tier is reachable from a test.
fn resolve_hosted_in(config: &Path, explicit: Option<&str>, here: Option<&str>,
    probe: &dyn Fn(&str) -> hostproto::HostProbe) -> Resolved;
```

### app/src/pane.rs (modified)

```rust
impl TerminalView {                                  // pane.rs:1786
    /// Beside new_restored (pane.rs:2455). Takes an ALREADY-BUILT replica
    /// Session + guard instead of spawning: same event pump, same generation
    /// counter, same selection/scroll — client state by construction.
    /// session.master is None here, so the 800ms /proc watcher task
    /// (pane.rs:2500) is NOT spawned; mode/exit arrive via set_host_mode /
    /// the workspace pump. The guard is checked on Push::GridCheck; Mismatch →
    /// loud log + Request::Resnapshot.
    pub fn new_attached(session: term::Session, guard: crate::gridwire::ReplicaGuard,
        pane_id: u64, restore: crate::session::PaneRestore, cx: &mut Context<Self>) -> Self;

    /// The host watcher's output, routed by the Workspace event pump. Replaces
    /// foreground_mode's local answer for attached panes. (Resolution: one
    /// draft passed &str — the typed hostproto::WireMode wins.)
    pub fn set_host_mode(&mut self, mode: hostproto::WireMode, cx: &mut Context<Self>);

    // field: /// Durable host pane id. None = serverless pane (has no durable id).
    // pub(crate) pane_id: Option<u64>,
}
```

### app/src/ctl.rs (modified)

```rust
pub(crate) fn ctl_dir() -> PathBuf;                  // was private (ctl.rs:313)

/// The identity the HOST stamped into this PTY's env. Read FIRST; the /proc
/// parent-walk (owning_td, ctl.rs:1031) stays verbatim as the serverless
/// fallback. Env survives tmux's re-parenting where the walk dies (#215).
struct StampedSeat { session: String, pane_id: u64 }
/// Some only when BOTH TD_SESSION and TD_PANE_ID are present and parseable.
fn stamped_seat() -> Option<StampedSeat>;

/// ctl-session-<id>.sock → ctl-<pid>.sock symlink in ctl_dir(): how a stamped
/// caller finds the attached WINDOW without /proc. Published by the Workspace
/// attach path, retired at detach/steal (only when it still points at this
/// pid); stale links swept on failed connect like every other socket here.
pub fn session_alias_path(session: &str) -> PathBuf;
pub fn publish_session_alias(session: &str, pid: u32) -> std::io::Result<()>;
pub fn retire_session_alias(session: &str, pid: u32);

enum Seat {
    Stamped { window: u32, pane_id: u64 },           // env + alias symlink; durable pane address
    Walked  { window: u32, pane_pid: Option<u32> },  // today's walk result (ctl.rs:1031)
}
/// stamped_seat() first, walk second. relay_target (ctl.rs:1053) and the tab
/// verbs resolve through this.
fn caller_seat() -> Option<Seat>;

pub(crate) enum TabRef {                             // ctl.rs:96
    Index(usize),
    Pane(u32),
    /// Durable pane id — translated window-side against TerminalView.pane_id.
    Durable(u64),
}
```

### scripts + bench (signatures of record)

```rust
// app/src/term.rs test module — the flip-gate instrument.
/// Env-driven: TD_ECHO_MODE=local|attached, TD_ECHO_FLOOD=0|8, TD_BIN=<path>.
/// local: term::spawn_in(cat); attached: term::attach_in against a spawned
/// `$TD_BIN serve` (skips LOUDLY until the attach slice lands — never fakes a
/// number). 1000 × { notifier.notify(b"x") — the exact GUI write path
/// (pane.rs:3436) — spin on term.lock() until the cursor cell echoes }. Prints
/// one JSON line {"mode","flood","p50_us","p99_us"}. Identical instrument both
/// modes: the diff isolates the two unix hops.
#[test] #[ignore = "latency bench — run via scripts/td-echo-bench.sh"]
fn echo_latency_bench();
```

`scripts/td-survival-test.sh {gui-kill|floor-control|host-kill|flood-stage} [--cycles N]`
— stages 4 panes (sentinel `seq` 3000 lines, vim, htop) via `ctl adopt` +
`leave_note` (mcp.rs:1006), sleeps past the 30s checkpoint, snapshots via
`mcp rpc list_panes` + `grep {scrollback:50000}`, then runs the leg; one JSON
summary line; exit 0 iff the leg's expectation holds.
`scripts/td-echo-bench.sh` — builds release, runs the bench over
mode×flood∈{local,attached}×{0,8}, gates p99(attached)−p99(local) ≤ 1000µs.

### docs/protocol/session-host-v1.md (new) — outline

1 Transport & auth — `session-<id>.sock` in the 0700 runtime dir; flock-guarded; auth is SO_PEERCRED uid at accept and NOTHING else. 2 Framing — control: NDJSON, one verb per line, replies + pushes interleaved; byte stream: one line `stream <pane_id>`, then raw unframed bytes both ways, FIFO per pane (the guard's offset arithmetic depends on it). 3 Handshake & versioning — hello is OPTIONAL (reversed 2026-09-10: it negotiates a version, it does not open a connection, and the peer-uid check at accept is the boundary); `version_check` rule; additive-within-major (unknown fields ignored, pinned by test); on a true break: Shutdown → checkpoint → TOML recovery — degrades to exactly today, once, deliberately. 4 Verbs — request/reply schemas with embedded JSON examples the conformance tests deserialize. 5 Pushes — mode/exit/detached/grid-check; per-conn, best-effort, never blocking the host. 6 Attach — attach-pane (resize+intent) then stream-connect (atomic splice); new attach wins THAT PANE (window-level steal deleted 2026-09-10); nothing assumes exactly one attached client; resize is told, last-writer-wins. 7 Close semantics — close-pane = SIGHUP the child tree; disconnect kills nothing. 8 Persistence — opaque schema-versioned envelope; host merges only per-leaf cwd/resume by pane_id; 30s checkpoint; shrink guard verbatim; host recounts leaves. 9 Invariants & tripwires — the FairMutex LEASE argument (re-verify on any alacritty upgrade); host pane table beats the TOML; ids never reused; scrollback never on disk. 10 Conformance — the test names pinning each section.

## Call stacks

**SLICE-0 DISPATCH** (ships first, no host exists)
```
main [main.rs:18333]
└─ dispatch(argv.get(1), |p| Path::new(p).is_dir())
   ├─ Launch::Verb(EmitDemo|Ctl|Mcp|AgentUsage|AgentVitals|Probe) → existing handlers, plain exit
   ├─ Launch::Verb(Serve) → serve::run_cli → serve::parse_args
   │     slice 0: refusal, exit 2 naming the later slice │ host slice: exit(host::run_serve(args))
   ├─ Launch::Reply{text,code} → print → exit  (flags via flag_reply main.rs:18320 unchanged;
   │     NEW arm: unknown positional → code 2 + USAGE — the #314 kill)
   └─ Launch::Window{open_here} → tty::setup_env → scratch decision → resolve_boot → GUI
```

**ATTACH** (TD_SESSIOND=1, the main path)
```
main → resolve_boot [scratch check unchanged, main.rs:18422-18444]
→ instance::resolve_session_hosted → resolve_hosted_in(probe = hostproto::probe_host, 250ms/candidate)
→ Route::SpawnHost ⇒ hostctl::spawn_host: setsid `terminal-delight serve --session <id>`
     host: tty::setup_env → instance::claim_in [instance.rs:471, fail-closed]
           (loser: probe Live → exit 0 / else exit 1) → instance::bind [instance.rs:86]
           → UnixListener::bind(host_socket_path) → watcher/checkpoint/guard threads → accept_loop
→ HostHandle::attach_gui: Hello{proto:1, Gui, build} — STEALS any prior GUI conn (decision 1)
→ Workspace::build(…, Some(AttachCtx)) [main.rs:2481]
   → load_state [main.rs:1168] + handle.list_panes → plan_attach (pure)
   → LeafPlan::Bind ⇒ handle.attach_pane (host: Session::resize term.rs:85 → Reply::Attached)
        → handle.open_pane_stream → host: route_conn → splice_attach:
            steal old sink (TeeSinks::remove + Push::Detached{Superseded})
            → gridwire::fenced_attach: term.lease() [sync.rs:28] → term.lock_unfair() [sync.rs:41]
              → encode_snapshot(&mut term) → enqueue → register sink → guards drop
            → writer thread drains AttachReceipt.chunks → socket; spawn input_pump
        client: term::attach_in → EventProxy{suppress_pty_write:true} → SocketPty
            → EventLoop::new/spawn [vendored event_loop.rs:63/:205] → (Session, ReplicaGuard)
            → TerminalView::new_attached [beside pane.rs:2455]
     LeafPlan::Respawn ⇒ handle.spawn_pane(cwd, resume, geom)
        host: term::spawn_hosted (env: TD_SESSION+TD_PANE_ID) → types resume line → drain_events thread
        → then the Bind path above
   → plan.orphans → one new Tab each (Tab::new, main.rs:648) — ORPHAN ADOPTION, pinned
   → ctl::publish_session_alias(session, pid)
   → handle.take_events → pump: Push::Mode ⇒ set_host_mode · Push::Exit ⇒ remove leaf + save
       · Push::GridCheck ⇒ ReplicaGuard::check (NotYet ⇒ re-poll; Mismatch ⇒ loud log + resnapshot)
       · Push::Detached ⇒ tear down that replica · Lost ⇒ attach.lost=true, alias retired
```

**KEYSTROKE** (steady state; echo path included)
```
key → pane.rs send [pane.rs:3436] → replica Session.notifier.notify → Msg::Input
→ replica EventLoop pty_write [vendored event_loop.rs:174] → SocketPty::writer (UnixStream)
→ host input_pump → host Session.notifier.notify [event_loop.rs:335] → Msg::Input
→ host EventLoop writer → PTY master
… kernel tty echo …
→ host pty_read [event_loop.rs:104]: lease :117 → TeeReader::read :122
   (File::read(master) → try_send to each live sink; full ⇒ dead=true, pane unharmed)
→ try_lock_unfair :140 → Processor::advance into authoritative Term :154
→ EventProxy → Host::drain_events (PtyWrite ⇒ host bounces to notifier — the term.rs:7-9 contract)
tee'd bytes → client CountingReader::read (offset += n) → replica parse → generation bump → repaint
RESIZE (never on the byte stream): GUI resize → replica Session::resize → SocketPty::on_resize
→ send_resize closure → Request::Resize → host Session::resize → TIOCSWINSZ [unix.rs:406]
```

**CLOSE-PANE** (intent)
```
✕ / ClosePane action → Workspace → handle.close_pane → Request::ClosePane
→ Host::close_pane: libc::kill(-(shell_pid), SIGHUP) → Msg::Shutdown → panes.remove
   → drop Session (master close = today's drop-is-close, main.rs:77-82) → Reply::Closed{Ok}
→ (child exit also surfaces) Push::Exit → GUI removes leaf → Workspace::save → handle.save
→ last pane gone && ever_spawned → Host::shutdown(0): persist_now → instance::release
   [instance.rs:98, disarm-before-unlock verbatim] → unlink socket → exit
Tab close: same, fanned per leaf. Disconnect-without-ClosePane kills NOTHING.
```

**APP-QUIT** (disconnect only)
```
quit → attached: final Workspace::save → handle.save (host merges capture, persist_primary,
   truthful PersistOutcome) → drop control conn + pane streams
→ on_app_quit does NOT call instance::release — the hosted GUI never held the flock
→ host: sink EOFs → panes marked detached; checkpoint_loop keeps running (ledger-absent mode)
→ next launch: resolve_session_hosted tier 2 finds this host (attached_gui:false) → ATTACH stack
```

**HOST-CRASH-RECOVERY** (the never-worse gate; version break lands on the same path)
```
kill -9 the host → per pane: socket EOF → CountingReader writes eof_pipe →
   run-loop child-event arm [event_loop.rs:256-271] → ChildEvent::Exited(None) → replica loop exits
→ control conn EOF → HostEvent::Lost → attach.lost=true (save inert — a lost window never
   clobbers the TOML), alias retired
→ relaunch: tier 1/2 probe → NoSocket (dead host's socket fails connect; flock is free)
→ tier 3: most-recent TOML → SpawnHost → fresh host claims flock, seeds layout from disk
→ plan_attach(saved, live=[]) ⇒ every leaf Respawn — today's respawn-and-type recipe,
   typed host-side; scrollback and vim/htop lost = EXPECTED-EQUAL-TO-FLOOR, proven by the
   harness host-kill leg's deep-equal against the floor-control snapshot
Version break variant: Hello → Reply::VersionMismatch → client sends Request::Shutdown
→ host persist_now → exit → same recovery code as above — one deliberate restart.
```

## Test plan

**Slice 0 — dispatch (main.rs unit + e2e)**
- `known_verbs_dispatch_before_the_gui` — dispatch classifies ctl/mcp/agent-usage/agent-vitals/probe/serve/--td-emit-demo as `Launch::Verb` with `is_dir` never consulted.
- `an_existing_directory_positional_reaches_the_window` — `dispatch(Some(dir), |_| true) == Window{open_here: Some(dir)}`; `dispatch(None, …) == Window{open_here: None}`.
- `an_unknown_positional_exits_2_instead_of_a_window` — THE #314 pin, rewriting main.rs:17173: `"sevre"`, `"clt"`, `"open-sesame"`, `"/does/not/exist"` each yield `Reply{code:2}` naming the word + Usage; no `Window` for any unknown positional.
- `version_and_help_are_answered_without_opening_a_window` + `an_unrecognised_flag_is_refused…` (main.rs:17151/:17163) — KEPT verbatim: flag behaviour unchanged by the rewrite.
- `a_typoed_verb_leaves_no_window_and_no_disk_writes` (app/tests/dispatch_cli.rs, real binary, tempdir HOME/XDG) — exit 2 within seconds, stderr names the word, tempdir contains no terminal-delight dir: the phantom-window class loses window AND writes.
- `serve_without_a_session_id_is_refused` (e2e) — `["serve"]` exits 2 naming `--session` (distinct from the unknown-verb usage: proves dispatch entered serve::run_cli).

**Wire (hostproto unit + conformance)**
- `hello_and_reply_round_trip_as_single_ndjson_lines` — Hello and every Reply/Push variant serialize to one line and back equal.
- `unknown_hello_fields_are_ignored_within_a_major` — additive-within-major pinned; new fields never force a bump.
- `a_version_mismatch_names_both_versions_and_build_never_gates` — `version_check(2,1) == Err(VersionMismatch{host:1,client:2})`; `(1,1) == Ok`; two Hellos differing only in `build` both pass.
- `the_protocol_doc_examples_deserialize` — every ```json fence in session-host-v1.md parses into a hostproto type: the host is conformance-tested against the DOCUMENT.
- `peercred_gate_is_the_only_auth` — same-uid connect passes; the refusal branch drops before any protocol read; schema assertion that no verb carries identity.

**Host (integration, temp XDG dirs, no gpui)**
- `host_headless_spawn_and_tee_roundtrip` — hello→Reply::Hello{proto:1}; spawn-pane → PaneId(1) + live shell_pid; stream connect, write `echo td-sentinel`, read tee'd bytes back; list-panes reports attached:true, generation advanced.
- `env_stamping_reaches_the_child` — `/proc/<shell_pid>/environ` contains TD_SESSION=<key> and TD_PANE_ID=1 (the stale instance.rs export comment made true).
- `splice_under_flood_loses_and_duplicates_nothing` — cat-flood + N attach/detach cycles: replay(snapshot)+tee'd bytes into a fresh Term equals the authoritative grid every cycle (the judge-found race stays dead; the lease tripwire's integration pin).
- `steal_on_attach_drops_the_previous_holder` — second stream connect: first stream EOF, its control conn gets Push::Detached{Superseded}; second stream's bytes reach the PTY; host still running.
- `close_pane_sighups_the_tree_and_reports_truthfully` — pane running `sleep 500` under the shell: Closed{Ok{signalled:true}}; shell and sleep both ESRCH within grace; disconnect-without-close kills NOTHING.
- `save_merge_is_opaque_envelope_safe` — body with an unknown future field (`tabs[0].task_group="x"`) and a Leaf with pane_id=1: written TOML keeps the unknown field verbatim; that leaf's cwd/resume are live capture; a leaf WITHOUT pane_id untouched.
- `shrink_guard_outcome_is_truthful` — 1-pane Save over an on-disk 6-pane session, allow_shrink:false → Saved{Ok(RefusedShrink{…})}, disk unchanged + .last-good written.
- `host_recount_feeds_the_shrink_guard` — the host's own walk (not any client claim) supplies old/new leaf counts; an envelope whose top-level `panes` int lies does not defeat the guard.
- `checkpoint_runs_with_no_client` — drop every connection, wait one period: TOML mtime advances, leaf carries fresh cwd.
- `exit_push_and_last_pane_teardown` — shell `exit` → Push::Exit{Some(0)}; pane removed; host exits 0; flock released only after the final persist (instance.rs:98 ordering pinned).
- `hello_version_mismatch_refuses_without_side_effects` — proto:999 → VersionMismatch; no pane spawned, no file written; garbage first line → Error, never a fall-through.
- `sanity_cap_refuses_truthfully` — spawn #65 → Spawned{Err} naming the cap; the 64 untouched (an 8-pane legacy restore is far below — decision 5 honoured).
- `mode_classification_and_sticky_rule` (pure) — claude→Claude, ssh→Remote, bash→Shell; Claude + non-agent fg + ALT_SCREEN stays Claude; normal screen demotes (parity with pane.rs:2516-2526).
- `two_serve_races_resolve_by_flock` — two concurrent run_serve: exactly one holds flock+socket; the loser exits 0 after probing the winner; no second socket, no clobbered TOML.
- `host_is_its_own_session_leader` — the spawned host's /proc sid equals its own pid, and it survives SIGHUP + exit of the process that spawned it. (kill -9 on the GUI never tests this: -9 delivers no HUP, so a host that failed to setsid would pass every other test and die later on a real terminal close or logout — the exact loss class the feature exists to prevent.)
- `cli_hello_leaves_the_gui_attached` — with a Gui attached, a probe_host / Cli-kind hello + list-panes leaves attached:true and the Gui's streams intact: only a Gui-kind attach steals. (resolve_session probes every candidate with kind:Cli; if Cli ever stole, every window launch would detach the live window.)
- `suppressed_pty_write_still_bumps_generation` — a replica EventProxy with suppress_pty_write set: Event::PtyWrite forwards nothing to the socket AND generation still advances; host integration: a DA query through an attached pane produces exactly one reply on the PTY (the single-answer contract kept; a regression here doubles every vim DA/DSR reply or silently breaks the invalidation token).
- `detached_host_backs_off_and_reattach_restores_cadence` — with no client attached, watcher and checkpoint cadences slow to 5s / 5min; an attach restores 800ms / 30s within one period.

**Gridwire (unit + property)**
- `roundtrip_plain_scrollback` — 300 sentinel lines into an 80×24 harness Term (term.rs:204-210), encode, replay via the feed() pattern (term.rs:218): cell-by-cell equality over history+screen, cursor equal, hashes equal.
- `roundtrip_sgr_and_wide_extremes` — 256-colour + truecolor, bold/italic/underline/strikeout/inverse, underline_color, CJK wide + spacer flags, zerowidth marks, soft-wrap at last column: WRAPLINE re-derived, hashes equal.
- `roundtrip_alt_screen_vim_shape` — alt-attach encodes ?1049h + alt paint only: replica shows the byte-identical alt screen; then ?1049l through the tee plus the alt-exit re-snapshot → primary + full scrollback converge, hashes equal (the heal path, end to end).
- `encode_touches_nothing` — encode takes &Term (read-only at the type level) and encoding twice — plain, alt, and mid-flood Terms — yields byte-identical output with grid_hash unchanged before and after: the neutrality the falsified double-swap design could only claim.
- `roundtrip_modes_cursor_charsets` — DECSCUSR shapes, SHOW_CURSOR off, bracketed paste, app-cursor, SGR mouse, DEC charset in G0: all equal after replay.
- `hash_excludes_client_state` — PageUp scroll and a set selection change neither grid_hash nor the verdict.
- `fence_no_byte_lost_or_duplicated` (property, 100 seeded iterations) — a writer floods through a TeeReader wired to a pty_read-style lease→read→lock_unfair→parse loop while N fenced_attach calls land at random offsets: snapshot-replay + subsequent chunks reproduce the host grid exactly — no byte in both, none in neither.
- `resnapshot_converges_in_band` — corrupt the replica by one byte → Mismatch; fenced_resnapshot; after drain → Match — and Session/EventLoop never rebuilt (same Arc pointers).
- `guard_not_yet_is_not_a_mismatch` — GridCheck delivered while read_offset < stream_offset → NotYet, never Mismatch; after catch-up → Match.
- `socket_eof_exits_replica_loop` — closing the host end terminates the replica EventLoop via the eof_pipe arm (join succeeds, CPU time bounded); Exited(None) exactly once.
- `slow_sink_dies_pane_survives` — a full SyncSender marks the sink dead on the next read; the read still returns all bytes to the parser; is_dead reports it so Push::Detached{SlowConsumer} can fire.

**GUI attach + persistence (unit, hermetic)**
- `resolve_hosted_prefers_live_unattached_host_over_newer_toml` — probe stub: live-unattached session 2 beats session 3's newer TOML → AttachLive on 2 (the kill-relaunch case).
- `explicit_td_session_wins_every_hosted_tier` — explicit id returned regardless of other hosts; Live→AttachLive (steal), NoSocket→SpawnHost.
- `orphan_host_pane_lands_in_a_new_tab` — saved claims pane 1, host reports 1+2 → orphans=[2]; zero live panes absent from the plan (host table beats the TOML, pinned).
- `dead_pane_id_falls_back_to_respawn_recipe` — leaf{pane_id:Some(7)}, host=[] → Respawn carrying cwd+resume — never a dropped leaf, never a Bind to a dead id.
- `duplicate_pane_id_claims_bind_once` — two leaves claiming 5: first Binds, second Respawns; 5 not in orphans.
- `legacy_overcap_layout_loads_every_pane` — a 6-leaf tree (legal under old cap) yields 6 panes; split() on that tab refuses at MAX_PANES=4; only NEW splits enforce 4.
- `saved_node_pane_id_roundtrips_and_legacy_reads_none` — Some(3) round-trips; a pre-split TOML leaf reads None, never Some(0).
- `save_routes_to_host_and_never_writes_locally_when_attached` — AttachCtx present: one Save verb, instance::state_path() untouched on disk; RefusedShrink surfaces in stderr as main.rs:1258 does today.
- `save_is_inert_after_host_loss` — lost=true: save sends nothing AND writes nothing (the instance.rs corpse-clobbering doctrine extended to steal).
- `persist_primary_relocated_guard_is_byte_identical` — the existing richness-guard tests (main.rs:17240 et al.) pass against session::persist_primary unchanged.
- `saved_counts_reads_shallow_and_absent_is_none` — panes=6/3 tabs → Some(6)/Some(3) without deserialising SavedNode; pre-field file → None, not Some(0); missing file → both None.
- `stamped_seat_beats_parent_walk` — env + alias present → Stamped with zero /proc reads (hermetic per the instance.rs:1069 lesson); either var absent → Walked via today's owning_td.
- `session_alias_publish_retire_and_stale_sweep` — retire removes only when it still points at this pid (a stealer's fresh alias survives the loser's retire); dangling alias swept on failed connect.

**Harness + flip gates (scripts)**
- `echo_latency_bench` (#[ignore], via td-echo-bench.sh) — p50/p99 over 1000 notify→grid round-trips; gate: p99(attached,flood)−p99(local,flood) ≤ 1000µs for flood∈{0,8} (decision 2's 1ms gate; flood held at 8 panes as the stress/legacy case).
- `td-survival-test.sh gui-kill --cycles 20` — THE metric: per cycle after kill -9 + relaunch: 4 panes back, shell pids UNCHANGED, vim+htop pids unchanged and alive, sentinel lines 000001 AND 003000 found at scrollback:50000, tab indices unchanged, sticky note intact; losses == 0 over N=20 or exit 1 listing every failure.
- `td-survival-test.sh floor-control` — instrument self-test: same scene on today's path reads losses == cycles (a harness reporting 0 on today's build is itself broken) and records the post-TOML-recovery state as control.json.
- `td-survival-test.sh host-kill` — kill -9 the serve host mid-run, relaunch: recovered state deep-equals control.json field-for-field — "exactly today's TOML respawn" proven by diff, not argument; scrollback/vim/htop loss asserted equal-to-floor, never counted as a metric loss.
- `td-survival-test.sh flood-stage` — a hand-written legacy 8-pane TOML (no pane_id fields, over the new cap) loads with all 8 panes running cat floods (decision 5's loader rule end-to-end); the stage stays up for the no-visible-lag eyeball while the bench supplies the number.

## Follow-ups filed at tie-off

Escalated as falsifiable GitHub issues when this feature ties off — the house
rule; recorded here so no graft silently drops between gates:

- The `grid.frame` / `grid.lines` generation-stamped read verbs — the named
  post-v1 door to presenting beyond a classic terminal UI, addable on the
  authoritative Term without re-cutting the replica seam (Gate 2 graft 7).
- Push-feed closure for the mcp relay path; the #308 write-gating
  unification; tear-off attach-by-id; per-pid ctl socket retirement (Gate 2
  fork F — all refused as unfunded by the metric, none forgotten).

## Least confident decisions

1. **The TeeReader double-fd shape.** TeeReader reads a `try_clone` of the PTY master while polling registration stays on inner Pty's fd — same open file description so readiness and reads should agree, but this needs a Linux smoke test first; the fallback (delegate to `inner.reader()` and tee at the same callsite) keeps the tee inside the leased cycle, so atomicity is unaffected either way.
2. **SIGHUP to `-(shell_pid)`** assumes shell_pid == its pgid (alacritty's child does setsid) and that vim/htop in their own foreground pgid die via the master-close HUP that follows drop; the nested-child close test is the decider — may need an extra kill to tcgetpgrp's group.
3. **toml 0.8 `Value` round-trip** preserving unknown nested fields (and enough key order not to churn diffs) is assumed for the opaque-envelope merge; if it reorders badly, switch to `toml_edit` — wire and signatures unchanged.
4. **The splice-at-stream-connect ordering** (attach-pane = resize+intent on control; fence at the byte-stream connect) is a chosen contract; a resize landing between the two legs is only caught by the divergence guard. If the client domain finds the two-step racy, folding geom into the `stream` line is a compatible v1 change.
5. **Host recounting leaves from the envelope walk** (no counts on the wire) could misfeed the shrink guard on an exotic future layout shape whose leaves the walk misses; the walk counts only Leaf tables, which #319 must preserve — pinned by `host_recount_feeds_the_shrink_guard`, but the coupling to #319's schema discipline is real.
6. **`drain_events` teardown**: block_on over the UnboundedReceiver in a plain thread is adequate live, but Msg::Shutdown / EventLoop join vs receiver drop ordering could leak an EventLoop thread per closed pane — needs deliberate teardown ordering.
7. **Exit-on-last-pane-close armed by `ever_spawned`** may race a GUI mid-restore that closes pane 1 before spawning pane 2; a short linger grace (or a client hint in a later proto rev) may be needed — the kill-relaunch harness will show it.
8. **Resize-vs-bytes reflow divergence**: host applies a resize at its byte position, the replica at an earlier one; transient reflow differences will trip the guard. The re-snapshot is the designed net, but how often pane-drag resizing fires loud re-snapshots is unmeasured — measure at Gate 4 before tuning the quiescence window.
9. **The alt-screen attach leg** was redesigned after the existence verifier falsified the original double-swap_alt mechanism against the vendored source — swap_alt's mode toggle is unconditional (term/mod.rs:732), so one encode on a vim pane would have erased its live alt grid; caught before any code existed. The replacement (alt-only paint at attach, fenced re-snapshot pushed on alt exit) is safe by construction, and its heal timing is now the least-proven line: the ?1049l reaching the replica races the host's re-snapshot push, and the fence must order them. The guard catches a miss; how visible the flash is on a real vim exit is unmeasured until the attach slice.
10. **polling 3.11 with UnixStream sources**: Pty registers `&File`; SocketPty registers UnixStreams. `add_with_mode` takes impl AsFd so it should type-check, but this is compile-verified nowhere yet — first task of the attach slice.
11. **TeeSink capacity** is designed (overflow ⇒ dead sink ⇒ forced re-attach) but the bound is not — pick it from the cat-flood measurement, not a guess.
12. **The ctl session-alias symlink** is an invention for resolving a Stamped seat to the attached window; the alternative is asking the host who is attached (one extra hop, no symlink hygiene). The `Seat` enum is unchanged either way — cheap to swap if the symlink's stale-sweep proves fiddly.
13. **Post-steal client behaviour**: the losing window freezes (dead replicas, save inert) because the product gate says no new UI — but auto-closing the stolen window may be what tmux-style steal should feel like. Needs Parker at Gate 4.
14. **Attached `build_state` emits cwd/resume as None**, relying on the host's merge to fill them; if the merge ever misses, the TOML recovery floor silently thins — `save_merge_is_opaque_envelope_safe` must keep asserting a saved attached layout still carries resumable recipes.
15. **Two checkpoint writers**: the client 30s checkpoint is kept (it carries window bounds/tab names the host never learns) funneling into the host serializer beside the host's own 30s capture checkpoint; last-writer-wins on the envelope needs one stated rule or the two cadences will interleave stale layouts.
16. **`TabRef::Durable` translation** assumes every ctl tabs-op consumer can reach `TerminalView.pane_id` at dispatch time; the full tabs-op dispatch in ctl.rs/main.rs was not re-read to confirm no path resolves a TabRef before a Workspace exists.
17. **Harness relaunch under Hyprland**: the script assumes `$TD_BIN &` yields the window process as `$!` and that a script-relaunched GUI inherits a usable Wayland env; the first live run must confirm both or switch to pgrep-by-session + `hyprctl dispatch exec`.
18. **Serde wire spellings** (`#[serde(tag=…)]` + variant casing) must be frozen in session-host-v1.md before the host slice codes against them — the doc-examples conformance test exists precisely to catch a drift here.