# Gate 3 fleet — domain drafts and verification record (2026-09-08)

> Four domain drafts written blind to each other, integrated into
> 03-program-design.md, then adversarially verified. This file is the record:
> the raw drafts (including cross-domain needs the integrator resolved) and
> every verification finding, ok and not. The not-ok findings are folded into
> the design; the fold is described in 03-program-design.md's header.

## Draft: session-host-and-wire — the `serve` verb, the per-session gpui-free host process, its pane table and PTY ownership, the NDJSON control protocol + per-pane byte streams on session-<id>.sock, steal-on-attach, SIGHUP close, relocated 800ms mode watcher and 30s checkpoint, TD_SESSION/TD_PANE_ID stamping, and the docs/protocol contract page

### Files

- `/home/parker/Work/terminal-delight/app/src/host.rs` (new) — The session host runtime: Host + HostPane + TeePty, accept loop with SO_PEERCRED, verb handlers, splice_attach (lease-then-lock), watcher/checkpoint threads, run_serve entry — gpui-free by module discipline, std threads like ctl.rs
- `/home/parker/Work/terminal-delight/app/src/hostproto.rs` (new) — The wire in one module: serde Request/Reply/Push/Outcome types, PaneId, WireMode, PROTO_VERSION, host_socket_path, and probe_host — the tiny sync client helper resolve_session's new tier and tests both use; pure serde+std so client and host share it
- `/home/parker/Work/terminal-delight/app/src/term.rs` (modified) — PTY spawn relocation without deleting spawn_in: split into spawn_pty (tty::new half, gains SpawnSpec.env for TD_SESSION/TD_PANE_ID via vendored tty::Options.env, tty/mod.rs:36) + wire_event_loop (Term+EventLoop half, generic over EventedPty so the host can pass TeePty); spawn_in (term.rs:105) becomes a 3-line composition of the two, signature unchanged
- `/home/parker/Work/terminal-delight/app/src/main.rs` (modified) — Slice 0: positional allowlist (unknown bare word exits 2, existing dir reaches the window) + rewrite of the pinning test at main.rs:17173; later slices: `serve` dispatch before flag_reply, `mod host; mod hostproto;`, persist_primary_state (main.rs:1250) returns PersistOutcome (truthful), MAX_PANES (main.rs:75) 8 -> 4
- `/home/parker/Work/terminal-delight/docs/protocol/session-host-v1.md` (new) — The one-page versioned contract the conformance tests run against (outline at the end of the signatures block)

### Signatures

```rust
// ============ app/src/hostproto.rs (new) — the wire, shared by host and client ============
// Pure serde + std. NDJSON: one serde_json line per Request/Reply/Push on the control
// connection; the byte-stream connection is raw bytes after one text line.

pub const PROTO_VERSION: u32 = 1;
pub const ENV_SESSION: &str = "TD_SESSION";   // stamped into every host-spawned PTY
pub const ENV_PANE_ID: &str = "TD_PANE_ID";   // (vendored tty::Options.env, tty/mod.rs:36)

/// Durable, host-minted pane identity. Shell pid is an attribute, never an address
/// (per the Gate 2 answer to recon question 2; shrinks the #311/#299/#272/#151 class).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PaneId(pub u64);

/// session-<key>.sock in the same 0700 runtime dir as ctl-<pid>.sock
/// (ctl.rs ctl_dir(), :312-318 — made pub(crate) so both modules share one answer).
pub fn host_socket_path(key: &str) -> PathBuf;

/// Wire-side pane mode; classification logic mirrored from pane.rs foreground_mode
/// (:203-217) + PaneMode::classify (:41-63), std-only so the host never imports pane.rs.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum WireMode { Shell, Claude, Codex, Remote, Other(String) }

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct PaneGeom { pub cols: u16, pub rows: u16, pub cell_width: u16, pub cell_height: u16 }

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "verb", rename_all = "kebab-case")]
pub enum Request {
    /// First line of every control connection. kind: "gui" | "cli" | "test".
    Hello { proto: u32, kind: String },
    ListPanes,
    SpawnPane { cwd: Option<String>, geom: PaneGeom },
    /// Declares intent + sets size; the atomic splice happens on the byte-stream connect.
    AttachPane { pane: PaneId, geom: PaneGeom },
    Resize { pane: PaneId, geom: PaneGeom },       // last-writer-wins; never on the byte stream
    ClosePane { pane: PaneId },                    // intent: SIGHUP the child tree
    /// Layout body is an OPAQUE schema-versioned envelope (client-owned data, #319-safe):
    /// the host merges only the per-leaf cwd/resume it owns, via toml::Value, never StateFile.
    Save { schema: u32, body: String, allow_shrink: bool },
    Shutdown,                                      // checkpoint, then exit — the version-break degrade path
}

/// Truthful per-target outcome — the UiReq::Apply shape (mcp_transport.rs:55,
/// mcp.rs:318 `pub type ApplyOutcome = (Target, Result<GradeReport, String>)`) on the new surface.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Outcome<T> { Ok(T), Err(String) }

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PaneInfo {
    pub pane: PaneId,
    pub shell_pid: u32,             // attribute (term.rs:68), not the address
    pub cwd: Option<String>,        // None = not yet captured — unknown is not ""
    pub resume: Option<String>,     // None = no derivable agent session (session.rs:176 chain)
    pub mode: WireMode,
    pub attached: bool,             // a byte-stream currently holds this pane
    pub exited: bool,
    pub geom: PaneGeom,
    pub generation: u64,            // the invalidation token (term.rs:69-73)
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "reply", rename_all = "kebab-case")]
pub enum Reply {
    Hello { proto: u32, session: String, panes: usize, attached: bool }, // attached feeds resolve_session's new tier
    Panes { panes: Vec<PaneInfo> },
    Spawned { outcome: Outcome<PaneInfo> },
    Attached { pane: PaneId, outcome: Outcome<PaneInfo> },
    Resized { pane: PaneId, outcome: Outcome<()> },
    Closed { pane: PaneId, outcome: Outcome<CloseInfo> },
    Saved { outcome: Outcome<SaveInfo> },
    ShuttingDown,
    Error { msg: String },          // unknown verb / parse error / proto mismatch — NEVER a fall-through
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CloseInfo { pub shell_pid: u32, pub signalled: bool }
#[derive(Serialize, Deserialize, Debug)]
pub struct SaveInfo { pub written: bool, pub refused_shrink: bool }

/// Host -> client pushes, interleaved on the control connection.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum Push {
    Mode { pane: PaneId, mode: WireMode },                    // the relocated 800ms watcher's output
    Exit { pane: PaneId, status: Option<i32> },               // Event::ChildExit (vendored event_loop.rs:263)
    Detached { pane: PaneId, reason: DetachReason },          // steal: you were superseded
}
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum DetachReason { Superseded }

#[derive(Debug)]
pub struct HostProbe { pub proto: u32, pub session: String, pub panes: usize, pub attached: bool }
/// Tiny sync client: connect + hello + one reply, bounded by timeout. None = no live host
/// (a dead host's socket fails connect; the flock makes stale files unambiguous).
/// resolve_session's new first-ranked tier (instance.rs:299) is built on this.
pub fn probe_host(key: &str, timeout: Duration) -> Option<HostProbe>;

// ============ app/src/term.rs (modified) — spawn relocation, spawn_in kept ============

/// Everything tty::new needs, plus the env the host stamps. spawn_in (term.rs:105,
/// signature unchanged) becomes spawn_pty + wire_event_loop with env=[].
pub struct SpawnSpec {
    pub size: GridSize,                    // term.rs:28
    pub cell_width: u16,
    pub cell_height: u16,
    pub cwd: Option<std::path::PathBuf>,   // vanished dir falls back, as today (term.rs:123)
    pub env: Vec<(String, String)>,        // -> tty::Options.env (vendored tty/mod.rs:36)
}

/// The tty::new half of today's spawn_in body (term.rs:115-141; vendored tty::new unix.rs:195).
/// TD_DEMO handling stays inside, exactly as term.rs:131-138.
pub fn spawn_pty(spec: &SpawnSpec) -> io::Result<alacritty_terminal::tty::Pty>;

/// The Term+EventLoop half (term.rs:142-158), generic so the host can pass TeePty.
/// Bounds are exactly EventLoop's own (vendored event_loop.rs:57-60:
/// T: tty::EventedPty + event::OnResize + Send + 'static). master/shell_pid are passed in
/// because a generic T has no .file()/.child() accessors (those are Pty's, unix.rs:110/:114).
pub fn wire_event_loop<T>(
    pty: T,
    size: GridSize,
    master: Option<std::fs::File>,
    shell_pid: u32,
) -> io::Result<Session>
where
    T: alacritty_terminal::tty::EventedPty + alacritty_terminal::event::OnResize + Send + 'static;

// ============ app/src/host.rs (new) — the session host ============

/// `terminal-delight serve --session <id>` entry. gpui is never initialized on this path.
/// Order: tty::setup_env() -> instance::claim_in(&instance::config_dir(), key) (instance.rs:471,
/// fail-closed verbatim; not owned -> if probe_host answers, exit 0 as the arbitration loser,
/// else exit 1) -> instance::bind (instance.rs:86) -> bind session-<key>.sock -> loops.
/// The SPAWNER setsids (Command::pre_exec(libc::setsid)); serve itself never forks.
pub fn run_serve(args: &[String]) -> i32;

/// One live pane: the authoritative side. `session` is a stock term::Session (term.rs:60) —
/// authoritative Term + Notifier + events + master + shell_pid + generation, built by
/// wire_event_loop over a TeePty.
pub(crate) struct HostPane {
    id: hostproto::PaneId,
    session: crate::term::Session,
    tee: Arc<Mutex<Option<TeeSink>>>,        // shared with the EventLoop thread's TeeReader
    geom: Mutex<hostproto::PaneGeom>,
    mode: Mutex<hostproto::WireMode>,
    exited: AtomicBool,
}

/// The host: owns the pane table, mints durable ids, holds the flock for its lifetime.
pub(crate) struct Host {
    key: String,
    panes: Mutex<BTreeMap<hostproto::PaneId, Arc<HostPane>>>,
    next_pane: AtomicU64,                    // ids monotonic per session, never reused
    ever_spawned: AtomicBool,                // exit-on-last-close arms only after the first spawn
    layout: Mutex<Option<(u32, toml::Value)>>, // (schema, opaque client envelope), disk-seeded at boot
    pushers: Mutex<Vec<PushHandle>>,         // every control conn; N clients is normal
}

/// Generous sanity cap only — the real per-window cap is the client's MAX_PANES=4
/// (main.rs:75). A legacy 5-8 pane layout must still restore through spawn-pane.
pub(crate) const HOST_PANE_SANITY_CAP: usize = 64;

impl Host {
    fn accept_loop(self: &Arc<Self>, listener: UnixListener) -> !;
    /// Auth is this check and nothing else; no protocol field carries identity.
    /// getsockopt(SOL_SOCKET, SO_PEERCRED) -> libc::ucred; uid != geteuid() -> drop before any read.
    fn peer_uid(stream: &UnixStream) -> io::Result<u32>;
    /// First line discriminates: '{' -> NDJSON control conn; "stream <pane_id>" -> byte stream.
    fn route_conn(self: &Arc<Self>, stream: UnixStream);
    fn control_loop(self: &Arc<Self>, stream: UnixStream);
    fn handle_request(self: &Arc<Self>, req: hostproto::Request, conn: &PushHandle) -> hostproto::Reply;

    fn spawn_pane(&self, cwd: Option<String>, geom: hostproto::PaneGeom)
        -> hostproto::Outcome<hostproto::PaneInfo>;   // stamps TD_SESSION+TD_PANE_ID via SpawnSpec.env
    fn attach_pane(&self, pane: hostproto::PaneId, geom: hostproto::PaneGeom)
        -> hostproto::Outcome<hostproto::PaneInfo>;   // resize first (term.rs:85), then wait for splice

    /// THE atomic attach splice, run on the byte-stream connection after its "stream <id>" line.
    /// Order is pty_read's own (vendored event_loop.rs:117 lease, :140 try_lock_unfair):
    ///   let _lease = pane.session.term.lease();      // sync.rs:28 — blocks a NEW read cycle
    ///   let term  = pane.session.term.lock_unfair(); // sync.rs:41 — lease held, fair lock() would self-deadlock
    /// A pty_read cycle in flight finishes first (its bytes are parsed before the lease drops:
    /// every break path in event_loop.rs:120-163 has unprocessed == 0), so under the guards the
    /// Term contains every byte read so far; write gridwire::encode_snapshot(&term) to the stream,
    /// register the TeeSink, drop guards — every byte in the snapshot or tee'd, never neither/both.
    /// Steal: an existing sink is shutdown(Both) and its control conn gets Push::Detached first.
    /// TRIPWIRE: this lease argument must be re-verified on any alacritty_terminal upgrade.
    fn splice_attach(&self, pane: &Arc<HostPane>, stream: UnixStream) -> io::Result<()>;

    /// close = intent (the approved split): SIGHUP the child tree
    /// (libc::kill(-(shell_pid as i32), SIGHUP)), Msg::Shutdown to the EventLoop, remove from the
    /// table, drop Session — the master-close HUP is today's drop-is-close (main.rs:77-82) kept.
    fn close_pane(&self, pane: hostproto::PaneId) -> hostproto::Outcome<hostproto::CloseInfo>;

    fn save(&self, schema: u32, body: String, allow_shrink: bool) -> hostproto::Outcome<hostproto::SaveInfo>;
    /// The relocated 30s checkpoint body, also the Save handler's tail: merge live capture
    /// into the envelope, then crate::persist_primary_state (main.rs:1250 -> shrink guard,
    /// rotate_state_backup, session::write_atomic session.rs:74). Host = the single TOML writer.
    fn persist_now(&self, allow_shrink: bool) -> crate::PersistOutcome;

    fn broadcast(&self, push: &hostproto::Push);
    fn watcher_loop(self: Arc<Self>);     // 800ms; classify + sticky-alt rule host-side
    fn checkpoint_loop(self: Arc<Self>);  // 30s -> persist_now(false)
    /// Consumes Session.events (term.rs:64). MUST bounce Event::PtyWrite into the notifier
    /// (the term.rs:7-9 contract — host owns the bounce; the replica must NOT double-answer).
    /// Event::ChildExit(status) -> Push::Exit + mark exited + remove pane.
    fn drain_events(self: Arc<Self>, pane: Arc<HostPane>, rx: UnboundedReceiver<TermEvent>);
    /// client bytes -> Notifier::notify (vendored event_loop.rs:335) -> Msg::Input -> PTY.
    fn input_pump(pane: Arc<HostPane>, stream: UnixStream);
    /// persist_now, Msg::Shutdown each EventLoop, instance::release() (instance.rs:98 —
    /// disarm-before-unlock ordering kept verbatim), unlink socket, exit.
    fn shutdown(&self, code: i32) -> !;
}

/// PTY wrapper the EventLoop is generic over (bounds verified: event_loop.rs:46,:57-60).
/// TeeReader::read copies to the sink (if any) then returns; a sink write error drops the
/// sink and the pane is merely detached — the host never dies of a client.
pub(crate) struct TeeSink { stream: UnixStream, pane: hostproto::PaneId }
pub(crate) struct TeeReader { file: std::fs::File, sink: Arc<Mutex<Option<TeeSink>>> }
impl std::io::Read for TeeReader { fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>; }
pub(crate) struct TeePty { inner: alacritty_terminal::tty::Pty, reader: TeeReader }
impl alacritty_terminal::tty::EventedReadWrite for TeePty {   // trait at vendored tty/mod.rs:65
    type Reader = TeeReader;
    type Writer = std::fs::File;
    // register/reregister/deregister/writer delegate to inner (Pty impl unix.rs:323)
}
impl alacritty_terminal::tty::EventedPty for TeePty {          // trait at vendored tty/mod.rs:92
    fn next_child_event(&mut self) -> Option<alacritty_terminal::tty::ChildEvent>;
}
impl alacritty_terminal::event::OnResize for TeePty {          // trait at vendored event.rs:98
    fn on_resize(&mut self, ws: alacritty_terminal::event::WindowSize);
}

/// Non-destructive merge over toml::Value (NEVER a StateFile round-trip — a newer client's
/// unknown #319 fields must survive the host untouched): walk tabs[*].node recursively,
/// update cwd/resume only on Leaf tables carrying pane_id, from session::capture
/// (session.rs:59) per live pane. Leaf counts for the shrink guard come from the same walk.
fn merge_capture_into_layout(
    layout: &toml::Value,
    live: &BTreeMap<hostproto::PaneId, crate::session::PaneRuntime>,  // session.rs:45
) -> toml::Value;

/// Relocated classification: tcgetpgrp + /proc comm/cmdline, logic from pane.rs
/// foreground_mode (:203-217) + PaneMode::classify (:41-63), returning the wire enum.
fn classify_foreground(master: &std::fs::File, shell_pid: u32) -> hostproto::WireMode;

// ============ app/src/main.rs (modified) ============

const MAX_PANES: usize = 4;   // main.rs:75, was 8 — per the amended decision 5 (client cap only)

/// SLICE 0 (ships alone, before any host exists): the #314 allowlist. Judged AFTER the six
/// headless verbs and flag_reply (main.rs:18337-18398): None -> window; an existing directory
/// -> window (the reserved open-here positional, implemented as validation); ANY other bare
/// word (`clt`, `sevre`, a nonexistent path) -> (usage, 2). `serve` joins the verb ladder in
/// the host slice: `if argv.get(1).map(String::as_str) == Some("serve") { exit(host::run_serve(&argv[2..])) }`.
fn positional_reply(first: Option<&str>) -> Option<(String, i32)>;
// The pinning test subcommands_and_bare_arguments_still_reach_the_window (main.rs:17173) is
// REWRITTEN in the same commit: existing dir reaches the window; typo'd verbs are refused.

/// persist_primary_state (main.rs:1250) becomes truthful — same guard, same writes, now a
/// reportable outcome so the host's Save reply and the GUI's eprintln both come from one fact.
pub(crate) enum PersistOutcome {
    Written,
    RefusedShrink { old_leaves: usize, new_leaves: usize, old_tabs: usize, new_tabs: usize },
    Io(String),
}
fn persist_primary_state(body: &str, new_leaves: usize, new_tabs: usize, allow_shrink: bool) -> PersistOutcome;

// ============ docs/protocol/session-host-v1.md (new) — one-page contract, outline ============
// 1 Transport & auth — session-<id>.sock in the 0700 runtime dir; flock-guarded; auth is
//   SO_PEERCRED uid at accept and NOTHING else (no identity field ever enters the protocol).
// 2 Framing — control conn: NDJSON, one verb per line, replies + pushes interleaved;
//   byte-stream conn: one text line `stream <pane_id>`, then raw unframed bytes both ways.
// 3 Handshake & versioning — hello{proto,kind} first; mismatch -> Error naming the supported
//   version; on a true break: client offers Shutdown, host checkpoints, TOML recovery runs —
//   degrades to exactly today, once, deliberately.
// 4 Verbs — the table above: request/reply schemas, per-target truthful outcomes.
// 5 Pushes — mode / exit / detached; delivery is per-control-conn, best-effort, never blocking
//   the host (a slow reader is disconnected, not waited on).
// 6 Attach & steal — attach-pane (resize+intent) then stream-connect (atomic splice); new
//   attach wins, old sink dropped + Push::Detached{superseded}; nothing assumes exactly one
//   attached client; resize is told, last-writer-wins.
// 7 Close semantics — close-pane = SIGHUP the child tree (intent); disconnect kills nothing.
// 8 Persistence — Save carries an opaque schema-versioned layout envelope; host merges only
//   per-leaf cwd/resume keyed by pane_id; 30s checkpoint; shrink guard verbatim.
// 9 Invariants & tripwires — the FairMutex lease argument (re-verify on any alacritty
//   upgrade); host pane table beats the TOML; ids never reused; scrollback never on disk.
// 10 Conformance — the test names in app/src/host.rs that pin each section.
```

### Call stacks

SERVE BOOT
main [main.rs:18333] → host::run_serve → alacritty_terminal::tty::setup_env [vendored tty/mod.rs:101] → instance::claim_in(&instance::config_dir(), key) [instance.rs:471, fail-closed] → (not owned: hostproto::probe_host → exit 0 loser / exit 1 broken) → instance::bind [instance.rs:86] → UnixListener::bind(hostproto::host_socket_path(key)) → spawn Host::watcher_loop + Host::checkpoint_loop threads → Host::accept_loop
  per conn: Host::peer_uid (SO_PEERCRED) → uid mismatch: drop → Host::route_conn → first byte '{' → Host::control_loop (thread) | "stream <id>" → Host::splice_attach + Host::input_pump (thread)

SPAWN-PANE
Host::control_loop → Request::SpawnPane → Host::spawn_pane → term::spawn_pty(&SpawnSpec{env: [(TD_SESSION,key),(TD_PANE_ID,id)]}) [term.rs new; body from term.rs:115-141; tty::new vendored unix.rs:195] → TeePty{inner: pty, reader: TeeReader{file: master dup, sink}} → term::wire_event_loop(teepty, size, master, shell_pid) [Term::new + EventLoop::new vendored event_loop.rs:63 + EventLoop::spawn :205] → panes.insert → spawn Host::drain_events(pane, session.events.take()) → Reply::Spawned{Ok(PaneInfo)}

ATTACH (host side; client does attach-pane then stream-connect)
Host::control_loop → Request::AttachPane → Host::attach_pane → Session::resize [term.rs:85] → Reply::Attached{Ok(PaneInfo)}
byte-stream conn → Host::route_conn → Host::splice_attach → steal: old TeeSink.shutdown(Both) + Host::broadcast(Push::Detached{Superseded}) → term.lease() [vendored sync.rs:28] → term.lock_unfair() [sync.rs:41] → gridwire::encode_snapshot(&term) [cross-domain] → stream.write_all(snapshot) → tee.lock().replace(TeeSink) → drop guards → spawn Host::input_pump

OUTPUT (steady state, alacritty's own thread)
EventLoop::pty_read [vendored event_loop.rs:104: lease :117 → read via TeeReader::read :122 (copy to TeeSink, then return) → try_lock_unfair :140 → Processor::advance into the authoritative Term :154]

INPUT
client bytes → Host::input_pump → Notifier::notify [vendored event_loop.rs:335] → Msg::Input → EventLoop writer → PTY master

CLOSE-PANE (intent)
Host::control_loop → Request::ClosePane → Host::close_pane → libc::kill(-(shell_pid as i32), SIGHUP) → notifier.0.send(Msg::Shutdown) → panes.remove → drop Session (master close = today's drop-is-close, main.rs:77-82) → Reply::Closed{Ok(CloseInfo)} → last pane gone && ever_spawned → Host::shutdown(0)

SAVE / CHECKPOINT (host = single TOML writer)
Host::control_loop → Request::Save{schema, body, allow_shrink} → toml::Value::from_str → layout.replace → Host::persist_now → session::capture per pane [session.rs:59: fg_pgid/tcgetpgrp + agent_resume chain :176-232] → host::merge_capture_into_layout → crate::persist_primary_state [main.rs:1250 → is_catastrophic_shrink :1189 → rotate_state_backup :1207 → session::write_atomic session.rs:74] → Reply::Saved{Ok(SaveInfo)}
Host::checkpoint_loop (30s) → Host::persist_now(false)  [same tail; runs with zero clients attached]

MODE WATCHER (relocated 800ms)
Host::watcher_loop (800ms) → per pane: host::classify_foreground(master, shell_pid) [logic from pane.rs:203+:41] → sticky-alt: keep agent mode while term.lock().mode().contains(TermMode::ALT_SCREEN) [rule from pane.rs:2516-2526, now host-side so pushes are flicker-free] → changed → Host::broadcast(Push::Mode)

EXIT PUSH + PTYWRITE BOUNCE
Host::drain_events → TermEvent::PtyWrite(text) → Notifier::notify (the term.rs:7-9 contract — host owns the bounce) | TermEvent::ChildExit(status) [emitted vendored event_loop.rs:263] → pane.exited=true → Host::broadcast(Push::Exit) → panes.remove → last gone → Host::shutdown(0)

SHUTDOWN VERB (version-break degrade)
Host::control_loop → Request::Shutdown → Host::persist_now(false) → Reply::ShuttingDown → per pane Msg::Shutdown → Host::shutdown(0) → instance::release [instance.rs:98, disarm-before-unlock verbatim] → unlink socket → exit

SLICE 0 DISPATCH (ships first, no host exists)
main [main.rs:18333] → six verb gates :18337-18386 → flag_reply :18320 → positional_reply [new: existing dir → None (window); any other bare word → exit 2] → GUI boot

### Test plan

- **dispatch_allowlist_refuses_unknown_positionals (rewrites main.rs:17173 pinning test, same commit — slice 0)** — flag_reply unchanged for flags; positional_reply(None)=None; positional_reply(existing dir e.g. std::env::temp_dir())=None (reaches the window); positional_reply("clt"), ("sevre"), ("/no/such/dir") = Some((usage, 2)); after the host slice, "serve" is dispatched before the gate and never reaches it
- **host_headless_spawn_and_tee_roundtrip (integration, temp XDG_RUNTIME_DIR/XDG_CONFIG_HOME, no gpui)** — run host in-process; hello→Reply::Hello{proto:1}; spawn-pane returns PaneId(1)+live shell_pid; byte-stream connect for the pane, write "echo td-sentinel\n", read back tee'd bytes containing td-sentinel; list-panes reports attached:true, generation advanced
- **env_stamping_reaches_the_child** — /proc/<shell_pid>/environ of a spawned pane contains TD_SESSION=<key> and TD_PANE_ID=1 (the stale instance.rs export comment made true)
- **splice_under_flood_loses_and_duplicates_nothing (cross-domain with gridwire; the lease tripwire's pin)** — with a cat-flood running, N attach/detach cycles: replay(snapshot)+tee'd bytes into a fresh Term equals the authoritative grid every cycle — no missing sentinel line, no doubled one (the judge-found race stays dead)
- **steal_on_attach_drops_the_previous_holder** — second byte-stream connect for the same pane: first stream hits EOF, its control conn receives Push::Detached{superseded}; bytes written on the second stream reach the PTY; host still running
- **close_pane_sighups_the_tree_and_reports_truthfully** — pane running `sleep 500` under the shell: ClosePane → Reply::Closed{Ok{signalled:true}}; shell and sleep both gone (kill(pid,0)==ESRCH) within grace; pane absent from list-panes; disconnect (drop conns without ClosePane) kills NOTHING — shell still alive
- **save_merge_is_opaque_envelope_safe** — Save body containing an unknown future field (e.g. tabs[0].task_group="x") and a Leaf with pane_id=1: written TOML still contains the unknown field verbatim, and that leaf's cwd/resume are the host's live capture; a leaf WITHOUT pane_id is untouched
- **shrink_guard_outcome_is_truthful** — Save with a 1-pane body over an on-disk 6-pane session, allow_shrink:false → Reply::Saved{Ok{written:false, refused_shrink:true}}, on-disk file unchanged + .last-good written (persist_primary_state semantics main.rs:1250 preserved)
- **checkpoint_runs_with_no_client** — spawn pane, drop every connection, wait one checkpoint period: session TOML mtime advances and the pane's leaf carries fresh cwd (host is the single writer, ledger-absent mode)
- **exit_push_and_last_pane_teardown** — shell `exit` → Push::Exit{status:Some(0)} on the control conn; pane removed; host process exits 0; flock released only after the final persist (release ordering instance.rs:98 pinned)
- **hello_version_mismatch_refuses_without_side_effects** — hello{proto:999} → Reply::Error naming version 1; no pane spawned, no file written, connection closed; a garbage first line likewise → Error, never a fall-through
- **peercred_gate_is_the_only_auth** — peer_uid(stream) returns geteuid() for a same-uid connect; the refusal branch (expected-uid parameter injected) drops before any protocol read; no verb carries identity (schema assertion over hostproto)
- **sanity_cap_refuses_truthfully** — spawn-pane #65 → Reply::Spawned{Err} naming the cap; existing 64 panes untouched (an 8-pane legacy restore is far below it — decision 5 honored: the cap is generous, the window cap of 4 lives client-side)
- **mode_classification_and_sticky_rule (pure)** — classify_foreground fake-/proc table: claude→Claude, ssh→Remote, bash→Shell; sticky: Claude + non-agent fg + ALT_SCREEN → stays Claude; normal screen → demotes (parity with pane.rs:2516-2526)
- **two_serve_races_resolve_by_flock** — two concurrent run_serve for one key: exactly one holds the flock and the socket; the loser exits 0 after probing the winner; no second socket, no clobbered TOML

### Least confident

- TeeReader reads from a dup of the PTY master while polling registration stays on inner Pty's fd — same open file description so readiness and reads agree, but the double-fd shape needs a smoke test on Linux; fallback is delegating to inner.reader() and teeing at the same callsite (both keep the tee inside the leased pty_read cycle, so atomicity is unaffected)
- SIGHUP to -(shell_pid) assumes shell_pid == its pgid (alacritty's child does setsid, unix.rs) and that vim/htop in their own foreground pgid die via the master-close HUP that follows drop — the close_pane test with a nested child is the decider; may need an additional kill to tcgetpgrp's group
- toml 0.8 Value round-trip preserving unknown nested fields (and enough key order not to churn diffs) is assumed for the opaque-envelope merge; if it reorders badly, switch the merge to toml_edit (new dep) — the wire and signatures don't change
- drain_events uses futures::executor::block_on over the UnboundedReceiver in a plain thread (the host has no executor); adequate for an unbounded channel but unproven under teardown ordering — Msg::Shutdown/EventLoop join vs receiver drop needs care to not leak an EventLoop thread per closed pane
- exit-on-last-pane-close armed by ever_spawned may still race a GUI mid-restore that closes pane 1 before spawning pane 2; a short linger grace (or an explicit client hint in a later proto rev) may be needed — kill-relaunch harness will show it
- the attach-pane (control: resize+intent) / stream-connect (splice) two-step is my ordering contract; a single-connection attach was not chosen — if the client domain finds the two-step racy for its EventLoop start, folding geom into the stream line is a compatible v1 change
- Save carries no leaves/tabs counts (host recounts from the parsed Value) — divergence between the client's count and the host's walk over an exotic future layout shape would misfeed the shrink guard; the walk counts only Leaf tables, which #319 must preserve

### Cross-domain needs (resolved by the integrator)

- gridwire (client/replica domain): pub fn encode_snapshot(term: &Term<EventProxy>) -> Vec<u8> — pure over &Term (scrollback + screen + cursor + modes + alt screen), callable while the host holds lease + lock_unfair; the host resizes the Term BEFORE snapshotting (attach-pane geom), so the encoder may assume current dimensions
- client attach_in / SocketPty (term.rs client half): the replica must NOT bounce Event::PtyWrite to the wire — the host owns the bounce (term.rs:7-9); a replica bounce would double-answer DA/DSR queries. Replica treats snapshot bytes as ordinary stream input; generation counter semantics unchanged
- persistence/layout domain: SavedNode::Leaf (main.rs:431) gains pane_id: Option<u64> (serde default, skip_serializing_if None — absent means pre-split file, unknown is not zero); my merge keys on exactly that field name
- layout loader domain: MAX_PANES=4 applies to NEW layouts only; a legacy saved layout holding 5-8 panes must load without losing a running pane (over-cap tolerated, orphan-adoption covers live panes the layout doesn't claim — host pane table beats the TOML on attach)
- instance/resolve domain: resolve_session (instance.rs:299) gains the first-ranked tier 'live host socket with no attached GUI' built on hostproto::probe_host(key, timeout), and a spawn-host helper that runs `terminal-delight serve --session <key>` with pre_exec(setsid) + detached stdio; losers of the flock race connect instead of exiting
- GUI attached-mode (main.rs/pane.rs): when attached, persist_primary_state calls become Request::Save over the control conn; on_app_quit must NOT call instance::release() for a flock the GUI no longer holds; the pane's 800ms local poll (pane.rs:2500) is replaced by Push::Mode, and Push::Detached tears down the replica (steal), Push::Exit drives today's exited handling
- ctl.rs: ctl_dir() (ctl.rs:312-318) becomes pub(crate) so hostproto::host_socket_path shares the one runtime-dir answer instead of a second derivation
- measurement domain: td-survival-test.sh gains the host-kill leg and drives kill-relaunch N=20 with sentinel scrollback + live vim/htop against this wire; flip gates (input-echo p99 within 1ms, no cat-flood lag at 8 panes) are measured through it, not promised

## Draft: attach seam and gridwire

### Files

- `/home/parker/Work/terminal-delight/app/src/term.rs` (modified) — The seam file: attach_in lands beside spawn_in (term.rs:105); SocketPty + TeePty live here because this is already the one alacritty-facing module (its header says written against the crate's public API); spawn_in is never deleted
- `/home/parker/Work/terminal-delight/app/src/gridwire.rs` (new) — Named by 02-architecture.md:36-38: snapshot encoder, grid hash, the lease fence, tee sink registry, divergence guard, and the round-trip property tests
- `/home/parker/Work/terminal-delight/app/src/main.rs` (modified) — One line: `mod gridwire;` module registration (this domain's only main.rs footprint; the dispatch allowlist is slice 0, another domain)
- `/home/parker/Work/terminal-delight/app/Cargo.toml` (modified) — Add `polling = "3"` — implementing EventedReadWrite requires naming polling::{Poller, Event, PollMode} in the impl (tty/mod.rs:72-74); Cargo.lock already resolves polling 3.11.0 via alacritty_terminal, it just isn't a direct dep yet

### Signatures

```rust
// ═══════════════════ app/src/term.rs (modified) ═══════════════════
// Ground truth this builds on (verified in the vendored crate at
// ~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/alacritty_terminal-0.26.0/src/):
//   EventedReadWrite { type Reader: io::Read; type Writer: io::Write;
//     unsafe fn register(&mut self, &Arc<Poller>, Event, PollMode) -> io::Result<()>;
//     fn reregister(..); fn deregister(..); fn reader(); fn writer() }   tty/mod.rs:65-78
//   EventedPty: EventedReadWrite { fn next_child_event(&mut self)
//     -> Option<ChildEvent> }  tty/mod.rs:92-97; ChildEvent::Exited(Option<ExitStatus>) :82-85
//   EventLoop::new(terminal, event_proxy, pty, drain_on_exit, ref_test)
//     where T: tty::EventedPty + event::OnResize + Send + 'static       event_loop.rs:57-69
//   FairMutex: lease() = next-slot only sync.rs:28; lock() = next THEN data :33-38;
//     lock_unfair() = data only :41-43; try_lock_unfair() :46-48
//   pty_read: lease held for the WHOLE cycle event_loop.rs:117; read :122 BEFORE
//     data lock :140/:142; parse :154; every loop exit leaves unprocessed == 0
//   Run-loop dispatch keys: PTY_READ_WRITE_TOKEN = 0 (unix.rs:32),
//     PTY_CHILD_EVENT_TOKEN = 1 (unix.rs:35) — pub(crate) NAMES, but the VALUES
//     are the contract the run loop matches on (event_loop.rs:258/:274)
//   Existing Session { term: Arc<FairMutex<Term<EventProxy>>>, notifier: Notifier,
//     events, master: Option<File>, shell_pid: u32, generation: Arc<AtomicU64> } term.rs:60-74

/// MODIFIED (tuple → named struct + one flag). A replica must SWALLOW
/// Event::PtyWrite: the host answers DA/DSR (it must — vim querying while
/// detached still needs replies, term.rs:7-9), and pane.rs:2963 bounces
/// unconditionally, so an unfiltered replica would double every query reply.
/// Generation still bumps on every event, suppressed or not (term.rs:45-49).
#[derive(Clone)]
pub struct EventProxy {
    tx: UnboundedSender<TermEvent>,
    generation: Arc<AtomicU64>,
    /// true only on replicas built by attach_in; spawn_in/spawn_hosted keep false.
    suppress_pty_write: bool,
}
impl EventListener for EventProxy { fn send_event(&self, event: TermEvent); } // shape as term.rs:53-58

/// What the GUI hands attach_in after the control-plane dance (hello →
/// attach-pane ack → byte-stream conn, `stream <pane_id>` preamble sent+acked;
/// the next bytes readable are the gridwire snapshot, then live tee'd PTY bytes).
pub struct AttachStreams {
    pub bytes: std::os::unix::net::UnixStream,
    /// Resize is a control verb, never a byte-stream fact (02-architecture.md:58).
    /// Called from SocketPty::on_resize on the EventLoop thread → Send + 'static.
    pub send_resize: Box<dyn FnMut(WindowSize) + Send + 'static>,
}

/// THE SEAM — beside spawn_in (term.rs:105). Stock EventLoop (event_loop.rs:63)
/// over SocketPty into a replica Term. Returns the SAME Session type, so all 33
/// pane.rs term.lock() sites (styled_lines pane.rs:5027, selection :4238,
/// scroll :4159, mirror_document :2312) run against the replica unchanged;
/// selection/scroll stay client state and the generation counter stays the
/// invalidation token BY CONSTRUCTION. master: None (term.rs:67 is already
/// Option — unknown is not zero; tcgetpgrp is meaningless client-side, mode
/// arrives as host-pushed events). shell_pid: host-reported attribute.
pub fn attach_in(
    size: GridSize,                                // term.rs:28; == attach-pane's cols/rows
    cell_width: u16,
    cell_height: u16,
    streams: AttachStreams,
    shell_pid: u32,
) -> io::Result<(Session, crate::gridwire::ReplicaGuard)>;

/// Client-side adapter: alacritty's public PTY traits over a unix stream —
/// EventLoop<T: tty::EventedPty, U> (event_loop.rs:46) is generic; no fork.
pub struct SocketPty {
    stream: std::os::unix::net::UnixStream,        // nonblocking; the fd the Poller watches
    reader: CountingReader,                        // over a try_clone of `stream`
    writer: std::os::unix::net::UnixStream,        // try_clone of `stream`
    /// Self-pipe mirroring tty::Pty::signals (unix.rs:105-106). On socket EOF,
    /// pty_read's `Ok(0) if unprocessed == 0 => break` (event_loop.rs:124) plus
    /// Level-mode polling would spin on a forever-readable closed fd; the
    /// reader writes one byte here instead, waking the run loop's child-event
    /// arm (event_loop.rs:256-271) which calls terminal.lock().exit() and breaks.
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
impl io::Read for CountingReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
}

impl tty::EventedReadWrite for SocketPty {         // tty/mod.rs:65
    type Reader = CountingReader;
    type Writer = std::os::unix::net::UnixStream;
    /// interest.key = 0 for `stream`, eof_pipe.0 as readable key 1 — mirrors
    /// Pty::register (unix.rs:328-346: add_with_mode on both sources).
    unsafe fn register(&mut self, poll: &Arc<polling::Poller>,
        interest: polling::Event, mode: polling::PollMode) -> io::Result<()>;
    fn reregister(&mut self, poll: &Arc<polling::Poller>,
        interest: polling::Event, mode: polling::PollMode) -> io::Result<()>;
    fn deregister(&mut self, poll: &Arc<polling::Poller>) -> io::Result<()>;
    fn reader(&mut self) -> &mut CountingReader;
    fn writer(&mut self) -> &mut std::os::unix::net::UnixStream;
}
impl tty::EventedPty for SocketPty {               // tty/mod.rs:92
    /// None until EOF observed; then Some(ChildEvent::Exited(None)) — exit
    /// status genuinely unknown here, and ChildEvent already models that
    /// (Option<ExitStatus>, tty/mod.rs:84). Authoritative pane exit arrives as
    /// a control-plane event (cross-domain).
    fn next_child_event(&mut self) -> Option<tty::ChildEvent>;
}
impl event::OnResize for SocketPty {               // event.rs:98-100
    fn on_resize(&mut self, window_size: WindowSize);  // → (self.send_resize)(ws)
}

// ── host side ──

/// spawn_in's hosted twin (spawn_in itself is NEVER deleted — scratch/demo/
/// serverless keep it). Same PTY spawn, but the EventLoop runs over TeePty so
/// live bytes fan out to attached clients. env carries the host-stamped
/// TD_SESSION/TD_PANE_ID (02-architecture.md:72; tty::Options.env, tty/mod.rs:36).
pub fn spawn_hosted(
    size: GridSize,
    cell_width: u16,
    cell_height: u16,
    cwd: Option<std::path::PathBuf>,
    env: Vec<(String, String)>,
) -> io::Result<(Session, crate::gridwire::TeeSinks)>;

/// Host-side wrapper: everything delegates to the real tty::Pty (unix.rs:102)
/// except Reader.
pub struct TeePty {
    inner: alacritty_terminal::tty::Pty,
    reader: TeeReader,
}
pub struct TeeReader {
    /// try_clone of inner.file() — same file description; the Poller watches the
    /// ORIGINAL fd (register delegates to inner). Same clone trick term.rs:140
    /// already uses for the mode-watcher master handle.
    master: std::fs::File,
    sinks: crate::gridwire::TeeSinks,
}
impl io::Read for TeeReader {
    /// read(master) → fan out via try_send to every live sink → return. Runs
    /// inside pty_read's lease (event_loop.rs:117→:122): MUST NOT block on a
    /// slow client — a full sink is marked dead (that client must re-attach →
    /// fresh fenced snapshot), the pane never stalls.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
}
impl tty::EventedReadWrite for TeePty {
    type Reader = TeeReader;
    type Writer = std::fs::File;                   // = Pty's Writer, unix.rs:325
    unsafe fn register(&mut self, poll: &Arc<polling::Poller>,
        interest: polling::Event, mode: polling::PollMode) -> io::Result<()>; // → inner, unix.rs:328
    fn reregister(&mut self, poll: &Arc<polling::Poller>,
        interest: polling::Event, mode: polling::PollMode) -> io::Result<()>; // → inner
    fn deregister(&mut self, poll: &Arc<polling::Poller>) -> io::Result<()>;  // → inner
    fn reader(&mut self) -> &mut TeeReader;        // the one non-delegating method
    fn writer(&mut self) -> &mut std::fs::File;    // → inner.writer(), unix.rs:377
}
impl tty::EventedPty for TeePty {
    fn next_child_event(&mut self) -> Option<tty::ChildEvent>;  // → inner, unix.rs:382-403
}
impl event::OnResize for TeePty {
    fn on_resize(&mut self, ws: WindowSize);       // → inner (TIOCSWINSZ), unix.rs:406
}

// ═══════════════════ app/src/gridwire.rs (new) ═══════════════════
//! Attach-time snapshot encoder, tee registry, THE LEASE FENCE, divergence guard.
//!
//! THE LEASE INVARIANT (the upgrade tripwire 02-architecture.md:87 demands, stated
//! per the panel's correction — the lease argument, NOT the lock argument):
//! pty_read holds FairMutex::lease() (next-slot only, sync.rs:28-30) for its whole
//! cycle (event_loop.rs:117), reads PTY bytes at :122 BEFORE the data lock
//! (:140/:142), and every exit of its loop leaves unprocessed == 0 — so when the
//! lease releases, every byte read has been parsed into the Term. A fence taking
//! lease() then lock_unfair() therefore (a) cannot interleave inside a read cycle,
//! (b) observes a Term containing exactly the bytes tee'd so far, (c) cannot
//! deadlock — identical acquisition order to pty_read. NEVER call FairMutex::lock()
//! while holding lease(): lock() re-takes next (sync.rs:36) and parking_lot
//! mutexes are not reentrant. Re-verify all four line-cites on any alacritty bump.

use alacritty_terminal::{event::EventListener, sync::FairMutex, term::Term};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SinkId(u64);

/// Registry shared between TeeReader (EventLoop thread) and the fence (serve
/// thread). Guarded by a plain std Mutex held only for push/register/remove —
/// never across a parse. Vec of sinks: NOTHING here assumes exactly one
/// attached client; steal is the serve loop's policy (remove then attach).
#[derive(Clone)]
pub struct TeeSinks(Arc<std::sync::Mutex<TeeState>>);
struct TeeState { sinks: Vec<TeeSink>, next_id: u64 }
struct TeeSink {
    id: SinkId,
    /// Bounded; try_send only. Overflow ⇒ sink marked dead, pane unharmed.
    tx: std::sync::mpsc::SyncSender<Arc<[u8]>>,
    /// Cumulative bytes ENQUEUED to this sink since attach — snapshot,
    /// re-snapshot and PTY bytes alike. The socket is FIFO, so this counter and
    /// the client's CountingReader tick the same clock; GridCheck.stream_offset
    /// quotes it, which is what makes host and replica hashes comparable.
    enqueued: u64,
    dead: bool,
}
impl TeeSinks {
    pub fn new() -> Self;
    pub fn remove(&self, id: SinkId) -> bool;      // steal / detach path
    pub fn is_dead(&self, id: SinkId) -> bool;     // overflow forces re-attach
}

/// What the serve loop gets back from a fenced attach; it drains `chunks` to
/// the byte-stream socket. Snapshot ordering is INHERENT: the snapshot is
/// enqueued into the same queue before the sink goes live.
pub struct AttachReceipt {
    pub sink: SinkId,
    /// Per-sink enqueued total right after the snapshot (== snapshot length).
    pub stream_offset: u64,
    pub cols: u16,
    pub rows: u16,
    pub chunks: std::sync::mpsc::Receiver<Arc<[u8]>>,
}

/// THE FENCE. Serve loop calls this on attach-pane AFTER applying the client's
/// size (resize-then-snapshot, so snapshot dims == replica dims; Session::resize,
/// term.rs:85). Fixed order inside:
///   1. let _lease = term.lease();          // blocks new pty_read cycles  sync.rs:28
///   2. let mut t  = term.lock_unfair();    // data lock, pty_read's order sync.rs:41
///   3. enqueue encode_snapshot(&mut t) into the new sink
///   4. register the sink live in `tee`
///   5. guards drop
/// ⇒ every PTY byte is in the snapshot XOR tee'd to this sink — never neither,
/// never both (02-architecture.md:84-88).
pub fn fenced_attach<T: EventListener>(
    term: &FairMutex<Term<T>>,
    tee: &TeeSinks,
    sink_capacity: usize,
) -> AttachReceipt;

/// Host half of the divergence guard: same fence, hash instead of encode.
/// Serve loop calls it at generation quiescence (host gen stable + sink queue
/// drained) and pushes the result as a control event. None if the sink is dead.
pub fn fenced_hash<T: EventListener>(
    term: &FairMutex<Term<T>>,
    tee: &TeeSinks,
    sink: SinkId,
) -> Option<GridCheck>;

/// The LOUD repair: under the same fence, enqueue RIS (ESC c — full reset,
/// history included) + a fresh snapshot into the surviving sink. In-band: the
/// replica's own parser resets and replays, so Session is never rebuilt and
/// pane.rs stays untouched. Returns the sink's enqueued total at injection
/// (the offset checks restart from). Err if the sink is dead.
pub fn fenced_resnapshot<T: EventListener>(
    term: &FairMutex<Term<T>>,
    tee: &TeeSinks,
    sink: SinkId,
) -> io::Result<u64>;

/// One integrity probe, host → client, over the control connection (NDJSON
/// framing is the protocol domain's).
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct GridCheck {
    pub pane_id: u64,
    /// Per-sink enqueued bytes at the fence (see TeeSink.enqueued).
    pub stream_offset: u64,
    pub hash: u64,
}

/// Snapshot encoder: authoritative Term → VT bytes reproducing scrollback +
/// screen + cursor + modes + alt screen in a fresh same-sized Term.
/// &mut because the alt-screen leg reaches the then-inactive primary grid via a
/// temporary DOUBLE Term::swap_alt (term/mod.rs:714): with ALT_SCREEN set the
/// destructive branch (:715-724, inactive_grid.reset_region) is skipped and
/// swap_alt does not toggle the mode bit, so the double flip is observationally
/// neutral — pinned by the encode-twice test. Emission: primary history+screen
/// top-to-bottom (rows Line(-history_size())..bottommost_line(), Dimensions,
/// grid/mod.rs:504-517; cells via grid[Line][Column] exactly as term.rs:224),
/// SGR runs from Cell {c, fg, bg, flags, underline_color(), zerowidth()}
/// (cell.rs:134-197), CR/LF only at non-WRAPLINE row ends so wrap flags
/// re-derive on replay; cursor via renderable_content().cursor (term/mod.rs:637,
/// :2365) + DECSCUSR from cursor_style() (:942); then, if ALT_SCREEN: CSI ?1049h
/// + alt paint + alt cursor; finally TermMode-derived modes (term/mod.rs:55:
/// SHOW_CURSOR, bracketed paste, app-cursor, mouse protocols, ALTERNATE_SCROLL)
/// and charset state (Grid.cursor.charsets, grid/mod.rs:42).
pub fn encode_snapshot<T: EventListener>(term: &mut Term<T>) -> Vec<u8>;

/// Deterministic cross-process content hash: FNV-1a 64 over a canonical
/// serialization (no HashMap iteration, no per-process seeds). COVERS every
/// history+screen row's cells (c, flags bits, fg, bg, underline_color,
/// zerowidth), cursor point, and ALT_SCREEN + input-relevant TermMode bits.
/// EXCLUDES display_offset and selection — client state by construction
/// (01-product.md multi-attach consequences); the replica may be scrolled or
/// selecting while converged.
pub fn grid_hash<T: EventListener>(term: &Term<T>) -> u64;

/// Client half of the guard, returned by term::attach_in alongside Session
/// (Session itself grows no fields; pane.rs never sees this type).
pub struct ReplicaGuard {
    term: Arc<FairMutex<Term<crate::term::EventProxy>>>, // clone of Session.term      term.rs:61
    generation: Arc<AtomicU64>,                          // clone of Session.generation term.rs:73
    read_offset: Arc<AtomicU64>,                         // shared with CountingReader
}
pub enum GuardVerdict {
    /// Replica hasn't consumed up to check.stream_offset yet, or its generation
    /// is still moving — re-poll later. NOT a mismatch: unknown is not zero.
    NotYet,
    Match,
    /// The loud path: caller logs both hashes and sends the resnapshot verb.
    Mismatch { host: u64, replica: u64 },
}
impl ReplicaGuard {
    /// Takes the replica FairMutex's OWN lease→lock_unfair fence (the same
    /// invariant, client side — the replica EventLoop is stock too), so every
    /// byte counted in read_offset is parsed before hashing.
    pub fn check(&self, check: &GridCheck) -> GuardVerdict;
    pub fn read_offset(&self) -> u64;
}
```

### Call stacks

ATTACH, host side (serve loop thread):
serve control loop `attach-pane {pane_id, cols, rows, cell_w, cell_h}` → Session::resize (term.rs:85: Msg::Resize to EventLoop + term.lock().resize) → gridwire::fenced_attach(term, tee, cap) → FairMutex::lease (sync.rs:28) → FairMutex::lock_unfair (sync.rs:41) → gridwire::encode_snapshot(&mut term) → enqueue snapshot into new TeeSink → TeeSinks register → guards drop → serve loop drains AttachReceipt.chunks → byte-stream socket write. Steal: serve loop calls TeeSinks::remove(old) + drops the old client's connection before fenced_attach.

LIVE BYTES, host side (alacritty "PTY reader" thread, stock):
EventLoop::run poll wake → pty_read (event_loop.rs:104) → term.lease() held (:117) → TeePty::reader() = TeeReader::read (:122) → File::read(master) → TeeSink.tx.try_send(chunk) per live sink, enqueued += n, full ⇒ sink.dead = true → try_lock_unfair (:140) → parser.advance into authoritative Term (:154) → EventProxy::send_event → host event pump (serve domain) bounces Event::PtyWrite via Session.notifier.

ATTACH, client side (GUI):
TerminalView::new_restored attach branch (GUI domain; pane.rs:2455) → term::attach_in(size, cw, ch, AttachStreams, shell_pid) → Term::new (term/mod.rs:410) + FairMutex::new + EventProxy{suppress_pty_write: true} → SocketPty{stream, CountingReader, eof_pipe, send_resize} → EventLoop::new(replica, proxy, socket_pty, false, false) (event_loop.rs:63) → EventLoop::spawn → Notifier(event_loop.channel()) → (Session, ReplicaGuard). Replica thread: pty_read → CountingReader::read (UnixStream read, offset += n; EOF ⇒ eof_tx write) → parser.advance into replica → EventProxy bumps generation → pane event pump unchanged (PtyWrite swallowed by proxy, never reaches pane.rs:2963).

INPUT:
keystroke → pane.rs send → replica Session.notifier.notify → Msg::Input → replica pty_write (event_loop.rs:174) → SocketPty::writer (UnixStream) → host stream loop reads → host Session.notifier.notify → Msg::Input → host pty_write → real PTY.

RESIZE:
GUI resize → replica Session::resize → replica term.lock().resize + Msg::Resize → drain_recv_channel (event_loop.rs:95) → SocketPty::on_resize → send_resize closure → control verb `resize` → host Session::resize → tty::Pty::on_resize (TIOCSWINSZ, unix.rs:406) + host term resize.

DIVERGENCE GUARD:
host quiescence timer (serve domain: host generation stable + sink drained) → gridwire::fenced_hash → lease → lock_unfair → grid_hash + read TeeSink.enqueued → GridCheck pushed on control conn → client event handler (GUI domain) → ReplicaGuard::check → read_offset < stream_offset or generation moving ⇒ NotYet (re-poll) → else replica lease → lock_unfair → grid_hash → compare → Mismatch ⇒ log loud + send `resnapshot {pane_id}` verb → host gridwire::fenced_resnapshot → lease → lock_unfair → enqueue RIS + encode_snapshot into the sink → replica parser resets + replays → next GridCheck matches.

### Test plan

- **roundtrip_plain_scrollback** — Feed 300 numbered sentinel lines into an 80x24 harness Term (harness() shape, term.rs:204-210), encode_snapshot, replay into a fresh same-sized Term via Processor::advance (the feed() pattern, term.rs:218); assert cell-by-cell equality over Line(-history_size())..bottommost_line() (c, flags, fg, bg, zerowidth), cursor point equal, and grid_hash(a) == grid_hash(b)
- **roundtrip_sgr_and_wide_extremes** — Corpus rows with 256-color + truecolor fg/bg, bold/italic/underline/strikeout/inverse, underline_color, CJK wide chars with WIDE_CHAR/WIDE_CHAR_SPACER flags, combining zerowidth marks, and a line soft-wrapped at the last column: replayed grid equal including Flags bits (WRAPLINE re-derived, not copied), hashes equal
- **roundtrip_alt_screen_vim_shape** — Primary with history → CSI ?1049h → full-screen paint + civis + cursor move: replay reproduces ALT_SCREEN mode bit, alt grid content, cursor; then ?1049l on BOTH terms shows identical primary + intact scrollback
- **encode_is_observationally_neutral_and_idempotent** — encode_snapshot called twice on the same alt-screen Term returns byte-identical output and leaves grid_hash unchanged (pins the double-swap_alt trick against term/mod.rs:714's reset_region branch and the keyboard_mode_stack swap); replay-then-encode equals the original encoding
- **roundtrip_modes_cursor_charsets** — DECSCUSR shapes, SHOW_CURSOR off, bracketed paste, application cursor keys, SGR mouse mode, DEC special charset in G0: every asserted TermMode bit and cursor_style equal after replay
- **hash_excludes_client_state** — scroll_display(Scroll::PageUp) and a set selection on the replica change neither grid_hash nor GuardVerdict — Match still returned while content is identical
- **fence_no_byte_lost_or_duplicated** — A writer thread floods bytes through a TeeReader wired to a FairMutex<Term> driven pty_read-style (lease→read→lock_unfair→parse loop mirroring event_loop.rs:117-154) while the test performs N fenced_attach calls at random offsets: for every attach, snapshot-replay + subsequent sink chunks reproduce exactly the host grid — no byte in both, none in neither, across 100 seeded iterations
- **resnapshot_converges_in_band** — Deliberately corrupt the replica (feed it one extra byte), run fenced_hash → ReplicaGuard::check == Mismatch; fenced_resnapshot; after the sink drains, check == Match — and Session/EventLoop were never rebuilt (same Arc pointers)
- **guard_not_yet_is_not_a_mismatch** — GridCheck delivered while read_offset < stream_offset returns NotYet, never Mismatch (unknown is not zero); after the replica catches up the same check returns Match
- **socket_eof_exits_replica_loop** — Closing the host end of the pair makes the replica EventLoop thread terminate via the eof_pipe child-event arm (join succeeds, no busy spin: thread CPU time bounded), and next_child_event returned Exited(None) exactly once
- **slow_sink_dies_pane_survives** — A sink whose SyncSender is full gets dead=true on the next TeeReader::read, the read still returns all PTY bytes to the parser, and TeeSinks::is_dead reports it so the serve loop can force re-attach

### Least confident

- Resize-vs-bytes ordering divergence: host applies a resize at its PTY-byte position, the replica applied it at an earlier stream position; alacritty reflow on different intermediate grids can differ transiently. The guard + re-snapshot is the designed net, but how often pane-drag resizing trips it (and thus how visible loud re-snapshots are) is unmeasured — Gate 4 should measure before tuning quiescence windows.
- The double-swap_alt encode path is verified against term/mod.rs:714-729 source (no mode toggle, no reset when ALT_SCREEN set), but the keyboard_mode_stack swap + set_keyboard_mode call inside swap_alt may emit state I haven't traced end-to-end; the encode-twice neutrality test exists precisely because this is the least-proven line of the encoder.
- polling 3.11 API for non-File sources: Pty::register uses poll.add_with_mode(&File,...); SocketPty registers UnixStreams. polling's add_with_mode takes impl AsFd/AsRawFd so it should type-check, but this is compile-verified nowhere yet — first slice task.
- TeeSink capacity number: overflow policy (kill sink, force re-attach) is designed, the bound is not — needs the cat-flood measurement from the flip gates (02-architecture.md:142-146, still measured at 8 panes) to pick a capacity that never trips in normal use.
- pty_read's hard-error exit (event_loop.rs:133) can return with read-but-unparsed bytes that WERE tee'd — on Linux EIO at hangup the host Term misses final bytes the replica parsed. Pane is dying anyway and no GridCheck fires post-exit, so I believe this is harmless; it is the one place the never-both-never-neither invariant bends, and it should be stated in the gridwire module doc.
- EventProxy tuple→named-struct change touches spawn_in's construction site only (term.rs:113) but any other construction of EventProxy I haven't found would break; a grep says there are none outside term.rs.

### Cross-domain needs (resolved by the integrator)

- HOST/SERVE domain: the attach-pane handler must call Session::resize BEFORE gridwire::fenced_attach (resize-then-snapshot so snapshot dims == replica dims), must implement steal by TeeSinks::remove(old_sink) + dropping the old byte-stream connection before the new fenced_attach, must drain AttachReceipt.chunks to the socket on a per-client writer task, and must run a host event pump that bounces Event::PtyWrite into Session.notifier (term.rs:7-9 contract — the HOST answers DA/DSR; replicas suppress theirs).
- HOST/SERVE domain: a quiescence scheduler that calls gridwire::fenced_hash when a pane's host generation is stable (suggest 500ms) and its sink queue is drained, pushes GridCheck as a control event, and services the `resnapshot` verb with gridwire::fenced_resnapshot; also forced re-attach signalling when TeeSinks::is_dead flips (slow-client overflow).
- PROTOCOL domain: control verbs attach-pane {pane_id, cols, rows, cell_w, cell_h} → {shell_pid, stream_offset}, resize, grid-check (GridCheck as an event), resnapshot {pane_id}; byte-stream preamble `stream <pane_id>` framing; guarantee the byte-stream connection is FIFO per pane (one connection per pane, 02-architecture.md:57-58) — the guard's offset arithmetic depends on it.
- GUI/CLIENT domain: TerminalView::new_restored's attach branch builds AttachStreams (connected pane stream + a send_resize closure into the control-connection writer), stores the ReplicaGuard beside the Session (one new TerminalView field — pane.rs grid sites stay untouched), runs ReplicaGuard::check on pushed GridChecks with NotYet re-polling, and sends the resnapshot verb + a loud user-visible log on Mismatch.
- GUI/CLIENT domain guarantee needed: mode/exit facts for attached panes come from host-pushed events, not tcgetpgrp — Session.master is None on attach and the 800ms watcher must not assume it (it is already Option<File>, term.rs:67).
- LAYOUT/PERSISTENCE domain: orphan adoption on attach must tolerate ANY live pane count — the MAX_PANES=4 cap (main.rs:75) applies to new-layout creation only; nothing in the attach seam or tee registry caps pane count, and a legacy 5-8 pane session must attach whole.
- SLICE 0 (dispatch domain): no dependency either way — term.rs/gridwire.rs land dark (nothing calls attach_in/spawn_hosted until the host exists), so this domain is slice 1 and independently shippable behind zero flags.
- BUILD: app/Cargo.toml gains polling = "3" (lockfile already resolves 3.11.0 transitively); no parking_lot direct dep needed — lease guards are held in `let _lease` bindings, never named.

## Draft: GUI attach and persistence

### Files

- `/home/parker/Work/terminal-delight/app/src/hostctl.rs` (new) — Client half of the session-host control protocol (probe, spawn setsid host, hello, list-panes, per-pane stream, save) — std-only, no gpui, testable without a window
- `/home/parker/Work/terminal-delight/app/src/instance.rs` (modified) — resolve_session_hosted() adds the first-ranked live-host-socket tier beside the untouched resolve_session (instance.rs:299); claim_in/release stay verbatim for the host and legacy paths
- `/home/parker/Work/terminal-delight/app/src/main.rs` (modified) — MAX_PANES 8→4 + LEGACY_PANE_CEILING 8 (main.rs:75), SavedNode pane_id (main.rs:431/:459), TD_SESSIOND boot branch (main.rs:18333), Workspace attach ctx + plan_attach + orphan adoption (main.rs:2481/:2406), save routing (main.rs:3015), persist_primary_state thinned to a wrapper (main.rs:1250)
- `/home/parker/Work/terminal-delight/app/src/session.rs` (modified) — persist_primary/SavedCounts relocation beside write_atomic (session.rs:74) — the shrink guard becomes gpui-free so the host is the single TOML writer running today's exact guard
- `/home/parker/Work/terminal-delight/app/src/pane.rs` (modified) — TerminalView::new_attached beside new_restored (pane.rs:2455); pane_id field; the 800ms /proc watcher is skipped when master is None (mode arrives as host events instead)
- `/home/parker/Work/terminal-delight/app/src/ctl.rs` (modified) — stamped_seat (TD_SESSION+TD_PANE_ID) consulted before the /proc parent-walk in owning_td (ctl.rs:1031); session-alias socket symlink published by the attached window; TabRef::Durable (ctl.rs:96)

### Signatures

// ============================================================
// app/src/hostctl.rs (NEW) — GUI-side client of session-<id>.sock.
// std + libc only. Mirrors ctl::ctl_dir()'s 0700 runtime dir (ctl.rs:313).
// ============================================================

use std::io::{self, BufReader};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

/// $XDG_RUNTIME_DIR/terminal-delight/session-<id>.sock (same dir as ctl-<pid>.sock, ctl.rs:321).
pub fn host_socket_path(id: &str) -> PathBuf;

/// Probe outcome. Unknown is not zero: a socket that exists but will not answer
/// is Unresponsive, never "no host".
#[derive(Debug, PartialEq)]
pub enum HostProbe {
    Live { proto: u32, gui_attached: bool },
    NoSocket,
    Unresponsive,
}

/// Connect + `hello {kind:"probe"}` within `budget`; never blocks boot longer.
pub fn probe_host(id: &str, budget: Duration) -> HostProbe;

/// One live pane as the host reports it (`list-panes`).
/// shell_pid is a reported attribute, never identity (the #311/#299/#272/#151 class).
#[derive(Clone, Debug)]
pub struct HostPane {
    pub pane_id: u64,
    pub shell_pid: u32,
    pub cwd: Option<String>,     // host's session::capture (session.rs:59); None = not yet known
    pub resume: Option<String>,
}

/// Events the host pushes on the control connection (the relocated 800ms
/// watcher's output + child exits). Pumped into panes by the Workspace.
pub enum HostEvent {
    Mode { pane_id: u64, mode: String },     // string form of pane.rs foreground_mode
    Exited { pane_id: u64 },
    /// Control connection EOF/steal — this GUI was displaced (decision 1) or the host died.
    Lost,
}

pub enum SaveOutcome { Written, RefusedShrink { old_leaves: usize, new_leaves: usize } }

/// An attached control connection (post-hello, kind:"gui" — the host DROPS any
/// previous GUI connection on this hello: second attach steals, decision 1).
pub struct HostHandle { /* BufReader<UnixStream>, session: String, proto: u32 */ }

impl HostHandle {
    pub fn attach_gui(id: &str) -> io::Result<HostHandle>;
    pub fn session(&self) -> &str;
    pub fn list_panes(&mut self) -> io::Result<Vec<HostPane>>;
    /// Second connection to the same socket, first line `stream <pane_id>`;
    /// the returned stream is handed verbatim to term::attach_in (cross-domain).
    pub fn open_pane_stream(&mut self, pane_id: u64) -> io::Result<UnixStream>;
    /// spawn-pane {cwd,resume,cols,rows} — the host spawns AND types the resume line
    /// (the recipe of session::PaneRestore, session.rs:27, executed host-side).
    pub fn spawn_pane(&mut self, cwd: Option<&str>, resume: Option<&str>, cols: u16, rows: u16) -> io::Result<HostPane>;
    /// close-pane = intent: the host SIGHUPs the pane's child tree (main.rs:77-82 drop-is-close becomes a verb).
    pub fn close_pane(&mut self, pane_id: u64) -> io::Result<()>;
    pub fn resize(&mut self, pane_id: u64, cols: u16, rows: u16, cell_w: u16, cell_h: u16) -> io::Result<()>;
    /// The persistence redirect: opaque schema-versioned layout body + the counts
    /// the shrink guard needs. Truthful per-target outcome (the UiReq::Apply model,
    /// mcp_transport.rs:55) — RefusedShrink comes back, never a silent ack.
    pub fn save(&mut self, body: &str, new_leaves: usize, new_tabs: usize, allow_shrink: bool) -> io::Result<SaveOutcome>;
    /// Split the connection: a blocking reader thread feeding HostEvents to a channel.
    pub fn take_events(&mut self) -> futures::channel::mpsc::UnboundedReceiver<HostEvent>;
}

/// Spawn `terminal-delight serve --session <id>` setsid-detached (current_exe,
/// like spawn_seeded_window main.rs:18236), then wait bounded for hello.
/// The HOST claims the flock via instance::claim_in (instance.rs:471); if it
/// loses the race it exits and this fn re-probes and attaches to the winner —
/// two $TD_SESSION launches contend on the same kernel primitive as today.
pub fn spawn_host(id: &str, budget: Duration) -> io::Result<HostHandle>;

// ============================================================
// app/src/instance.rs — resolution grows one tier; nothing existing changes.
// ============================================================

/// How the resolved session is reached. Legacy keeps today's Claim (instance.rs:459).
pub enum Route {
    /// A live host answered hello with gui_attached:false (or an explicit
    /// $TD_SESSION with any live host — attach steals). GUI holds NO flock.
    AttachLive,
    /// No live host: spawn `serve --session <id>`, then attach.
    SpawnHost,
    /// TD_SESSIOND unset — today's claim-and-own path, byte-for-byte (instance.rs:299).
    Legacy(Claim),
}

pub struct Resolved { pub id: String, pub route: Route }

/// TD_SESSIOND=1 resolution. Tier order (panel fork resolution, 02-architecture.md Flow):
///   1. $TD_SESSION (explicit_session, instance.rs:232) — probe: Live→AttachLive, else SpawnHost.
///   2. FIRST-RANKED ADOPTION TIER: any session whose host socket answers hello
///      and reports no attached GUI — ordered by rank() (instance.rs:378: workspace
///      hint, substantial, newest) → AttachLive. This IS the kill-relaunch case.
///   3. most-recently-saved TOML with no live host (rank order) → SpawnHost
///      (the host claims; a legacy GUI holding the flock makes the spawn fail → next candidate).
///   4. fresh id (fresh_session, instance.rs:393 shape) → SpawnHost.
pub fn resolve_session_hosted() -> Resolved;

/// Injectable core, like resolve_session_in (instance.rs:309): probe_fn stands in
/// for hostctl::probe_host so every tier is reachable from a test.
fn resolve_hosted_in(
    config: &Path,
    explicit: Option<&str>,
    here: Option<&str>,
    probe: &dyn Fn(&str) -> hostctl::HostProbe,
) -> Resolved;

// ============================================================
// app/src/session.rs — the relocated single write chokepoint (gpui-free,
// callable by host and legacy GUI alike; beside write_atomic, session.rs:74).
// ============================================================

/// Shallow counts from an existing session TOML WITHOUT deserialising the layout
/// envelope: `panes` is already a top-level int (StateFile.panes, main.rs:1067,
/// read cheaply the way instance::toml_top_level_usize does at instance.rs:420);
/// tabs counted as toml::Value array length. None = pre-field/unreadable file —
/// unknown, never zero.
pub struct SavedCounts { pub leaves: Option<usize>, pub tabs: Option<usize> }
pub fn saved_counts(path: &Path) -> SavedCounts;

pub enum PersistOutcome { Written, RefusedShrink { old_leaves: usize, old_tabs: usize } }

/// RELOCATED VERBATIM from main.rs:1250 (persist_primary_state) with
/// is_catastrophic_shrink and rotate_state_backup: shrink guard, .last-good
/// snapshot, 10 rotated backups, then write_atomic (session.rs:74). The host
/// calls this; the legacy GUI calls it via the main.rs wrapper. Now RETURNS its
/// outcome instead of only eprintln-ing, so the host's save verb can report
/// truthfully over the wire.
pub fn persist_primary(path: &Path, body: &str, new_leaves: usize, new_tabs: usize, allow_shrink: bool) -> PersistOutcome;

// ============================================================
// app/src/main.rs — caps, SavedNode, boot, Workspace attach, save routing.
// ============================================================

/// New splits stop at 4 (decision round #5: "a window holds one task's panes").
/// Enforced ONLY at split() (main.rs:4682) and new-pane creation — never by the loader.
const MAX_PANES: usize = 4;                       // was 8, main.rs:75
/// What a LOADED tab may still hold: legacy files saved up to 8 panes and the
/// CRT warp shader renders 8 tubes. build_node (main.rs:2406) and orphan
/// adoption tolerate up to this; badge consts re-point here (MAX_TAB_BADGES=4
/// at main.rs:18100; its assert at main.rs:18102 and badge_overflow test at
/// main.rs:16501 now compare against LEGACY_PANE_CEILING).
const LEGACY_PANE_CEILING: usize = 8;
const _: () = assert!(MAX_PANES <= LEGACY_PANE_CEILING);

enum SavedNode {                                  // main.rs:431
    Leaf {
        // …existing six fields unchanged (appearance/cwd/resume/name/logo/note)…
        /// The host pane this leaf shows. None = written pre-split, or by a
        /// serverless window — unknown is not zero; never default to 0.
        #[serde(skip_serializing_if = "Option::is_none")]
        pane_id: Option<u64>,
    },
    Split { /* unchanged */ },
}
// LeafFields (main.rs:459) gains: #[serde(default)] pane_id: Option<u64>

/// Boot mode, decided in main() (main.rs:18333) after the scratch decision:
/// explicit_scratch keeps today's path untouched; TD_SESSIOND=1 selects Hosted.
enum Boot {
    Scratch { seed: Option<session::PaneRestore> },   // TD_SCRATCH/TD_SEED_*/TD_DEMO_STATE, unchanged
    Legacy { key: String, claim: instance::Claim },   // today's resolve_session (instance.rs:299)
    Hosted { handle: hostctl::HostHandle },           // resolve_session_hosted + spawn/attach loop
}
fn resolve_boot() -> Boot;

/// Everything Workspace needs while attached. gpui main thread only → Rc<RefCell>.
struct AttachCtx {
    handle: std::rc::Rc<std::cell::RefCell<hostctl::HostHandle>>,
    /// list-panes at boot; refreshed on host events.
    panes: Vec<hostctl::HostPane>,
    /// Set once HostEvent::Lost arrives: panes freeze, save() goes inert
    /// (the host still owns flock + TOML — a stolen window must never write).
    lost: std::cell::Cell<bool>,
}

/// PURE attach planning — the testable core of restore-under-a-host.
/// For each saved leaf: Bind (pane_id live on the host) or Respawn (pane_id
/// absent/dead → today's recipe, executed via spawn-pane). Every live host
/// pane no leaf claimed is an orphan → one new tab each. A pane_id claimed by
/// two leaves binds the first, respawns the second (ids are unique on a host).
enum LeafPlan {
    Bind { pane_id: u64 },
    Respawn { restore: session::PaneRestore },
}
struct AttachPlan {
    /// LeafPlan per leaf, in each tab's tree order (parallel to SavedTab.node leaves).
    tabs: Vec<Vec<LeafPlan>>,
    /// Live host panes the layout did not claim — each becomes a new Tab.
    /// THE INVARIANT (pinned by test): the host's live pane table beats the
    /// TOML; a checkpoint-stale layout can never lose a running pane.
    orphans: Vec<hostctl::HostPane>,
}
fn plan_attach(saved: &[SavedTab], live: &[hostctl::HostPane]) -> AttachPlan;

impl Workspace {                                  // main.rs:1975
    /// build (main.rs:2481) gains the attach ctx; None = scratch/demo/legacy,
    /// which run today's branches byte-for-byte.
    fn build(
        scratch: bool,
        demo: bool,
        seed: Option<session::PaneRestore>,
        attach: Option<AttachCtx>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self;

    /// The hosted restore: plan_attach over load_state() + list-panes, then per
    /// leaf make_pane_attached / make_pane_host_spawned, orphans appended as new
    /// tabs (named from the pane's cwd basename), then publish the ctl session
    /// alias. Serverless build_node (main.rs:2406) is untouched.
    fn build_attached(&mut self, saved: StateFile, window: &mut Window, cx: &mut Context<Self>);

    /// save (main.rs:3015) routes: attached → HostHandle::save (host is the
    /// single TOML writer; RefusedShrink mirrors today's eprintln + .last-good,
    /// done host-side); attached-but-lost → inert; else → persist_primary_state
    /// wrapper as today. scratch/released() guards (main.rs:3017-3025) unchanged.
    fn save(&self, cx: &App);
}

/// Attach one live host pane as a TerminalView: attach-pane + byte stream via
/// HostHandle::open_pane_stream → term::attach_in (cross-domain) → the same
/// subscription wiring as make_pane_restored (main.rs:2270).
fn make_pane_attached(
    pane: &hostctl::HostPane,
    saved: Option<&SavedNode>,        // appearance/name/logo/note re-applied as build_node does (main.rs:2406)
    attach: &AttachCtx,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Entity<TerminalView>;

/// Respawn-under-a-host: HostHandle::spawn_pane(recipe) then make_pane_attached.
fn make_pane_host_spawned(
    restore: session::PaneRestore,
    attach: &AttachCtx,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Entity<TerminalView>;

/// persist_primary_state (main.rs:1250) becomes a one-line wrapper:
/// session::persist_primary(&instance::state_path(), …) — kept so the ~6 call
/// sites and the richness-guard tests do not churn.
fn persist_primary_state(body: &str, new_leaves: usize, new_tabs: usize, allow_shrink: bool);

// ============================================================
// app/src/pane.rs — the attach twin of new_restored.
// ============================================================

impl TerminalView {                               // pane.rs:1786
    /// Beside new_restored (pane.rs:2455). Takes an ALREADY-BUILT replica
    /// Session (term::attach_in, cross-domain) instead of spawning: same event
    /// pump, same generation counter, same selection/scroll — client state by
    /// construction. session.master is None here, so the 800ms /proc watcher
    /// task (pane.rs:2500) is NOT spawned; mode/exit arrive via set_host_mode.
    pub fn new_attached(
        session: term::Session,                   // term.rs:60, master: None
        pane_id: u64,
        restore: crate::session::PaneRestore,     // logo/note re-applied as in new_restored
        cx: &mut Context<Self>,
    ) -> Self;

    /// The host's 800ms watcher output, routed by the Workspace event pump.
    /// Replaces foreground_mode's local answer for attached panes.
    pub fn set_host_mode(&mut self, mode: &str, cx: &mut Context<Self>);

    // field: /// Durable host pane id. None = serverless pane (has no durable id).
    // pub(crate) pane_id: Option<u64>,
}

// ============================================================
// app/src/ctl.rs — stamped identity before the parent-walk.
// ============================================================

/// The identity the HOST stamped into this PTY's env. Read FIRST; the /proc
/// parent-walk (owning_td, ctl.rs:1031) stays verbatim as the serverless
/// fallback. Env survives tmux's re-parenting where the walk dies (#215).
struct StampedSeat { session: String, pane_id: u64 }
/// Some only when BOTH TD_SESSION and TD_PANE_ID are present and parseable.
fn stamped_seat() -> Option<StampedSeat>;

/// ctl-session-<id>.sock → ctl-<pid>.sock symlink in ctl_dir() (ctl.rs:313):
/// how a stamped caller finds the attached WINDOW without /proc. Published by
/// the Workspace attach path, retired at detach/steal; stale links swept on
/// failed connect like every other socket here.
pub fn session_alias_path(session: &str) -> PathBuf;
pub fn publish_session_alias(session: &str, pid: u32) -> std::io::Result<()>;
pub fn retire_session_alias(session: &str, pid: u32);

enum Seat {
    /// From env: the window currently attached to `session` (via the alias
    /// symlink), pane addressed durably.
    Stamped { window: u32, pane_id: u64 },
    /// Today's walk result (ctl.rs:1031): window pid + the shell-pid pane.
    Walked { window: u32, pane_pid: Option<u32> },
}
/// stamped_seat() first, walk second. relay_target (ctl.rs:1053) and the tab
/// verbs resolve through this.
fn caller_seat() -> Option<Seat>;

pub(crate) enum TabRef {                          // ctl.rs:96
    Index(usize),
    Pane(u32),
    /// Durable pane id — translated window-side against TerminalView.pane_id.
    Durable(u64),
}

### Call stacks

ATTACH BOOT (TD_SESSIOND=1, the main path):
main() [main.rs:18333]
→ resolve_boot() — explicit_scratch check unchanged [main.rs:18422-18444]
→ instance::resolve_session_hosted() → resolve_hosted_in(config, explicit, here, probe)
   → hostctl::probe_host(id, 250ms) per candidate (tier 1 explicit; tier 2 live-unattached in rank() order [instance.rs:378]; tier 3 TOML-no-host; tier 4 fresh)
→ Route::SpawnHost ⇒ hostctl::spawn_host(id) — setsid `terminal-delight serve --session <id>`; host runs instance::claim_in [instance.rs:471]; loser-of-flock ⇒ re-probe + attach to winner
→ hostctl::HostHandle::attach_gui(id) — hello{kind:"gui"} STEALS any prior GUI (decision 1)
→ instance::bind(key, None) [instance.rs:86] — hosted GUI never holds the flock
→ application().run → Workspace::build(false, false, None, Some(AttachCtx), …) [main.rs:2481]
   → load_state() [main.rs:1168] → handle.list_panes()
   → plan_attach(saved.tabs, panes) — pure
   → per leaf: LeafPlan::Bind ⇒ make_pane_attached → handle.open_pane_stream(pane_id) → term::attach_in (cross-domain) → TerminalView::new_attached [pane.rs beside :2455]
              LeafPlan::Respawn ⇒ make_pane_host_spawned → handle.spawn_pane(cwd, resume) → make_pane_attached
   → plan.orphans: one new Tab each (Tab::new, main.rs:648) — ORPHAN ADOPTION, pinned
   → ctl::publish_session_alias(session, std::process::id())
   → handle.take_events() → cx.spawn pump: HostEvent::Mode ⇒ view.set_host_mode; Exited ⇒ remove leaf + save; Lost ⇒ attach.lost=true, alias retired

SAVE, ATTACHED (persistence redirect — host is the single TOML writer):
StickyChanged/PaneRenamed/PaintApplied/30s checkpoint [main.rs:2904] → Workspace::save [main.rs:3015]
→ scratch/released guards unchanged → attach.lost ⇒ return (inert)
→ build_state(cx) [main.rs:2968] — to_saved stamps pane_id per leaf; attached leaves have cwd/resume None (no master to capture from — the host merges its own)
→ toml::to_string → handle.save(body, pane_count, tabs.len, allow_shrink)
→ [host domain] merge per-leaf cwd/resume by pane_id → session::persist_primary → session::write_atomic [session.rs:74]
→ SaveOutcome::RefusedShrink ⇒ eprintln mirror of today's refusal [main.rs:1258]

SAVE, LEGACY (unchanged semantics, relocated body):
Workspace::save → persist_primary_state wrapper [main.rs:1250] → session::persist_primary(&instance::state_path(), …) → session::saved_counts → session::write_atomic

CLOSE PANE/TAB, ATTACHED (close is intent):
ClosePane/confirm_close → permit_shrink.set(true) → handle.close_pane(pane_id) per leaf → host SIGHUPs child tree → HostEvent::Exited → leaf removed → save

SPLIT (new-pane cap = 4; loader never capped):
Workspace::split [main.rs:4666] → leaves.len() >= MAX_PANES(4) ⇒ refuse [main.rs:4682] → attached ⇒ make_pane_host_spawned; serverless ⇒ make_pane_restored [main.rs:2270] (build_node [main.rs:2406] and plan_attach tolerate up to LEGACY_PANE_CEILING=8)

CTL/RELAY SEAT RESOLUTION (TD_PANE_ID before the walk):
run_mcp_cli [ctl.rs:1095] / run_cli tab verbs → relay_target [ctl.rs:1053] → caller_seat()
→ stamped_seat() (TD_SESSION+TD_PANE_ID) ⇒ session_alias_path(session) symlink → window pid; pane addressed as TabRef::Durable(pane_id), translated window-side against TerminalView.pane_id
→ else owning_td() /proc parent-walk [ctl.rs:1031], byte-for-byte today's

### Test plan

- **resolve_hosted_prefers_live_unattached_host_over_newer_toml** — resolve_hosted_in with a probe stub: session 2 answers Live{gui_attached:false}, session 3's TOML is newer with NoSocket → Resolved{id:"2", route:AttachLive}. The kill-relaunch case: a running host outranks TOML recency.
- **explicit_td_session_wins_every_hosted_tier** — explicit=Some("9") returns id 9 regardless of live hosts elsewhere; probe Live → AttachLive (attach steals), NoSocket → SpawnHost. Mirrors instance.rs:975's escape-hatch doctrine.
- **orphan_host_pane_lands_in_a_new_tab** — plan_attach(saved claiming pane 1, host reporting panes 1 and 2) → tabs[0]=[Bind{1}], orphans=[pane 2]; zero live panes absent from the plan. THE pinned invariant: host pane table beats the TOML, a stale checkpoint can never lose a running pane.
- **dead_pane_id_falls_back_to_respawn_recipe** — plan_attach(leaf{pane_id:Some(7), cwd, resume}, host=[]) → LeafPlan::Respawn carrying that cwd+resume — today's respawn-and-type recipe, never a dropped leaf, never a Bind to a dead id.
- **duplicate_pane_id_claims_bind_once** — two leaves claiming pane_id 5 → first Binds, second Respawns; pane 5 not in orphans.
- **legacy_overcap_layout_loads_every_pane** — a 6-leaf saved split tree (legal under the old MAX_PANES=8) → plan_attach/build_node yield 6 panes; split() on that tab refuses (>= MAX_PANES=4). Loader tolerates over-cap; only NEW splits enforce 4.
- **saved_node_pane_id_roundtrips_and_legacy_reads_none** — serialize Leaf{pane_id:Some(3)} → toml contains pane_id=3 → deserializes back Some(3); a pre-split TOML leaf (no field) → pane_id None, never Some(0). Unknown is not zero.
- **save_routes_to_host_and_never_writes_locally_when_attached** — with AttachCtx present, save() produces one host save verb carrying (body, leaves, tabs, allow_shrink) and instance::state_path() is untouched on disk; RefusedShrink outcome surfaces in stderr like main.rs:1258 does today.
- **save_is_inert_after_host_loss** — attach.lost=true → save() sends nothing AND writes nothing locally — a stolen/orphaned window must never clobber the TOML the host owns (the instance.rs:99 corpse-clobbering doctrine, extended to steal).
- **persist_primary_relocated_guard_is_byte_identical** — the existing richness-guard tests (main.rs:17240 richness_guard_blocks_unintended_collapse_only et al.) pass against session::persist_primary: shrink refused ⇒ RefusedShrink + .last-good written + on-disk body untouched; allow_shrink ⇒ Written + backup rotated.
- **saved_counts_reads_shallow_and_absent_is_none** — a file with panes=6 and 3 tabs → SavedCounts{leaves:Some(6), tabs:Some(3)} without deserialising SavedNode; a pre-panes-field file → leaves None (not Some(0)); missing file → both None.
- **stamped_seat_beats_parent_walk** — TD_SESSION+TD_PANE_ID set and alias symlink present → caller_seat()=Stamped{window from alias, pane_id} with zero /proc reads (hermetic: env + tmp dir injected, per the instance.rs:1069 lesson); either var absent → Walked via today's owning_td.
- **session_alias_publish_retire_and_stale_sweep** — publish creates ctl-session-<id>.sock → ctl-<pid>.sock; retire removes it only when it still points at this pid (a stealing window's fresh alias survives the loser's retire); a dangling alias is swept on failed connect.

### Least confident

- The ctl session-alias symlink (ctl-session-<id>.sock → ctl-<pid>.sock) is my invention for resolving a Stamped seat to the attached WINDOW — the approved design only says 'reads TD_PANE_ID before the /proc parent-walk'. The alternative is asking the session host who is attached (one extra hop, no symlink hygiene). Integrator should pick one; the Seat enum is unchanged either way.
- Post-steal client behavior: I froze the losing window (panes keep dead replicas, save inert) because the product gate says no new UI — but auto-closing the stolen window may be what tmux-style steal should feel like. Needs Parker at Gate 4.
- build_state on an attached client emits cwd/resume as None (no PTY master to capture from), relying on the host's merge to fill them. If the host's merge ever misses, the TOML's recovery floor silently thins — the host-domain merge test must assert a saved attached layout still carries resumable recipes.
- Whether the 30s client checkpoint (main.rs:2904) should keep running when attached, given the host runs its own capture checkpoint — I kept it (it carries window bounds/tab names the host never learns), so two writers funnel into one host serializer; if the host's own checkpoint also writes the layout envelope from its last-received body, ordering needs one rule.
- TabRef::Durable translation window-side assumes every ctl tab consumer can reach TerminalView.pane_id at dispatch time; I did not re-read the full tabs-op dispatch in ctl.rs/main.rs to confirm no path resolves TabRef before a Workspace is available.

### Cross-domain needs (resolved by the integrator)

- term/replica domain: pub fn attach_in(stream: UnixStream, size: GridSize, cell_width: u16, cell_height: u16, shell_pid: u32) -> io::Result<Session> — returns the SAME term::Session (term.rs:60) with master: None, events/notifier/generation live, driving the stock EventLoop (event_loop.rs:46 in alacritty_terminal-0.26.0) over a SocketPty implementing EventedReadWrite (tty/mod.rs:65) + EventedPty (tty/mod.rs:92). My new_attached consumes exactly this.
- Host domain: hello reply carries {proto: u32, gui_attached: bool} and a gui-kind hello DROPS the previous GUI connection (steal, decision 1) — my HostProbe and AttachCtx::lost depend on both.
- Host domain: list-panes rows are {pane_id: u64, shell_pid: u32, cwd: Option<String>, resume: Option<String>} with pane_id unique per host — plan_attach keys on it.
- Host domain: spawn-pane {cwd, resume, cols, rows} types the resume line into the fresh PTY host-side (the recipe semantics of pane.rs:2480-2482) and returns the new HostPane.
- Host domain: attach-pane snapshots under FairMutex::lease() then lock() in pty_read's order (sync.rs:28/:33, event_loop.rs:117-145 — verified in ~/.cargo/registry/.../alacritty_terminal-0.26.0) so every byte is in the snapshot or the tee, never neither; my attach path assumes the replica arrives complete.
- Host domain: the save verb treats the layout body as an opaque schema-versioned envelope EXCEPT the per-leaf cwd/resume it merges by pane_id, then calls session::persist_primary (my relocation) and returns the PersistOutcome truthfully — my SaveOutcome mirrors it.
- Host domain: stamps TD_SESSION + TD_PANE_ID into every PTY it spawns — stamped_seat() reads them; without the stamp the ctl improvement is dead code.
- Host domain: the host's own pane sanity cap must be >= LEGACY_PANE_CEILING (8) so adopting a legacy 5-8 pane session never kills a running pane (binding decision).
- Host domain: pushes mode/exit events per pane on the control connection (the relocated 800ms watcher's output) — my HostEvent::Mode/Exited pump and pane.rs set_host_mode consume them.
- Dispatch domain (slice 0): unknown positionals exit 2 and `serve` joins the ladder before flag_reply (main.rs:18320), with the main.rs:17173 pinning test rewritten — my hosted boot never depends on it, so slice ordering is preserved, but spawn_host invokes `serve` and needs it dispatched.
- Protocol doc domain: the one-page versioned contract under docs/protocol/ must name the verbs exactly as HostHandle speaks them (hello, list-panes, spawn-pane, attach-pane, resize, close-pane, save, shutdown, stream <pane_id>); my client is conformance-tested against that document.

## Draft: slice-0-and-proof-harness

### Files

- `/home/parker/Work/terminal-delight/app/src/main.rs` (modified) — dispatch allowlist replaces the if-ladder (18337-18386) and the #314 fall-through (18391-18398); pinning tests rewritten in the same inline test module that holds the old one (17172)
- `/home/parker/Work/terminal-delight/app/src/serve.rs` (new) — the serve verb's permanent entry signature so the allowlist has a real target before any host exists; slice-0 body is a refusal, not a GUI
- `/home/parker/Work/terminal-delight/app/src/proto.rs` (new) — hello/version handshake types + the one compatibility rule, beside ctl.rs/mcp.rs which own the other line protocols; shared by host, client, and conformance tests
- `/home/parker/Work/terminal-delight/docs/protocol/session-host-v1.md` (new) — the one-page versioned contract 02-architecture:59-60 mandates; conformance tests deserialize its embedded examples so the doc, not the impl, is the authority
- `/home/parker/Work/terminal-delight/app/tests/dispatch_cli.rs` (new) — end-to-end exit-code and no-disk-writes pins need the real binary (CARGO_BIN_EXE); first integration-test target in the crate — unit tests can't prove main() never mutates state
- `/home/parker/Work/terminal-delight/app/src/term.rs` (modified) — echo_latency_bench (#[ignore]) in the existing inline test module — it needs module access to spawn_in/Session, and the repo keeps tests inline
- `/home/parker/Work/terminal-delight/scripts/td-survival-test.sh` (new) — the metric instrument (lost sessions = 0), built at slice 0 per the panel graft so it exists before the behavior it measures; scripts/ is where operational harnesses live (release-smoke.sh precedent)
- `/home/parker/Work/terminal-delight/scripts/td-echo-bench.sh` (new) — flip-gate latency wrapper: runs the bench in both modes and both flood conditions, applies the ≤1ms jq gate, prints the numbers Parker signs

### Signatures

```rust
// ── app/src/main.rs — the dispatch allowlist (replaces main.rs:18337-18398) ──

/// Everything `main` may do with argv[1], decided by ONE pure function so the
/// whole contract is unit-testable without booting gpui. Kills #314: today an
/// unknown positional falls through flag_reply (main.rs:18320) into the GUI
/// and mutates on-disk session state (main.rs:18391-18466).
#[derive(Debug, PartialEq)]
enum Launch {
    /// Headless verb → its handler gets argv[2..], plain process exit, no gpui.
    Verb(Verb),
    /// Print and exit: code 0 → stdout, else stderr. Flags via flag_reply
    /// (main.rs:18320, unchanged); NEW arm: unknown positional → code 2 + USAGE.
    Reply { text: String, code: i32 },
    /// Boot the GUI. `open_here` is Some only for a positional naming an
    /// EXISTING directory — the reserved "open in this directory" slot,
    /// validated now instead of left as the #314 hole. Threaded as the
    /// fresh-session first-pane cwd; a session restore ignores it.
    Window { open_here: Option<std::path::PathBuf> },
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Verb {
    EmitDemo,    // "--td-emit-demo" → demo::emit_and_block()  demo.rs:27 (-> !)
    Ctl,         // → ctl::run_cli(&argv[2..]) -> i32          ctl.rs:873
    Mcp,         // → ctl::run_mcp_cli(&argv[2..]) -> i32      ctl.rs:1095
    AgentUsage,  // → usage::run_cli(&argv[2..]) -> i32        usage.rs:428
    AgentVitals, // → vitals::run_cli(&argv[2..]) -> i32       vitals.rs:1729
    Probe,       // → probe_cli(&argv[2..]) -> i32             main.rs:18260
    Serve,       // → serve::run_cli(&argv[2..]) -> i32        serve.rs (new)
}

/// The allowlist. Order: known verbs (incl. "--td-emit-demo") → `-`-leading
/// via flag_reply → existing-dir positional → refusal. `is_dir` is injected so
/// tests exercise the directory arm without a filesystem (the
/// scratch_decision test style, main.rs:17095+).
fn dispatch(first: Option<&str>, is_dir: impl Fn(&str) -> bool) -> Launch;

// flag_reply (main.rs:18320) is UNCHANGED; USAGE (main.rs:18290) gains the
// serve verb, the `terminal-delight <dir>` form, and the unknown-verb refusal.

// ── app/src/serve.rs — NEW: the verb's permanent entry (slice-0 stub body) ──

/// `terminal-delight serve --session <id>` — the per-session host verb, same
/// shape as ctl::run_cli (ctl.rs:873). Slice 0: parse args, print "the
/// session host lands in a later slice", exit 2 — the verb is RESERVED and
/// can never boot a GUI, while a typo'd `sevre` is refused by the allowlist.
pub fn run_cli(args: &[String]) -> i32;

/// `--session` is required — a host must never default to "whatever session
/// is newest" (the #311/#299/#272/#151 cross-wiring class).
pub struct ServeArgs {
    pub session: String,
}
pub fn parse_args(args: &[String]) -> Result<ServeArgs, String>;

// ── app/src/proto.rs — NEW: the version handshake at hello ──

/// Major version of the session-host control protocol. Bumped ONLY on a
/// breaking wire change; additive fields never bump it (serde ignores unknown
/// fields on deserialize — pinned by test). The contract of record is
/// docs/protocol/session-host-v1.md; conformance tests parse its examples.
pub const PROTO_VERSION: u32 = 1;

/// First line on every control connection, client → host, one NDJSON line
/// (the codebase's universal framing — ctl.rs / mcp_transport.rs precedent).
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
pub struct Hello {
    pub proto: u32,
    pub kind: ClientKind,
    /// Caller's CARGO_PKG_VERSION — logs/status only, NEVER gates: `proto`
    /// is the sole compatibility fact.
    pub build: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum ClientKind {
    /// A window attaching. A second Gui hello STEALS: the host drops the
    /// previous Gui connection (decision round #1) — never a refusal.
    Gui,
    /// ctl / scripts / the survival harness — attach-neutral, never steals.
    Cli,
}

/// First line back, host → client.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
#[serde(tag = "hello", rename_all = "kebab-case")]
pub enum HelloReply {
    Ok {
        proto: u32,
        /// The session id this host serves — echoes serve --session.
        session: String,
        /// Ranking fact for resolve_session's live-host tier
        /// (02-architecture:31-32) — a hint, not a lock: steal semantics mean
        /// nothing here assumes exactly one attached client.
        attached_gui: bool,
        /// Host's CARGO_PKG_VERSION so status surfaces can SHOW binary skew
        /// even while the protocol still matches.
        build: String,
    },
    /// Incompatible. The client's mandated next move: send `shutdown` (host
    /// checkpoints via its own persist loop, exits), then run today's TOML
    /// respawn recovery — degrades to exactly today, once, deliberately
    /// (02-architecture:61-63).
    VersionMismatch { host: u32, client: u32 },
}

/// The one compatibility rule, out of handler bodies so both ends and the
/// conformance tests share it: equal major ⇒ Ok, else the refusal naming both.
pub fn version_check(client: u32, host: u32) -> Result<(), HelloReply>;

// ── app/src/term.rs test module — the flip-gate instrument (#[ignore]) ──

/// Env-driven: TD_ECHO_MODE=local|attached, TD_ECHO_FLOOD=0|8, TD_BIN=<path>.
/// local: term::spawn_in (term.rs:105) with shell=cat; attached:
/// term::attach_in against a spawned `$TD_BIN serve` (skips loudly until the
/// attach domain lands). 1000 × { notifier.notify(b"x") — the exact GUI write
/// path (pane.rs:3436) — then spin on term.lock() until the cursor cell echoes }.
/// Prints one JSON line {"mode","flood","p50_us","p99_us"}. Identical
/// instrument both modes: the diff isolates the two unix hops.
#[test] #[ignore = "latency bench — run via scripts/td-echo-bench.sh"]
fn echo_latency_bench();
```

### Call stacks

DISPATCH (slice 0, ships alone):
main (main.rs:18333)
└─ dispatch(argv.get(1).map(String::as_str), |p| Path::new(p).is_dir())   [new pure fn]
   ├─ Launch::Verb(EmitDemo)   → demo::emit_and_block()               (demo.rs:27, diverges)
   ├─ Launch::Verb(Ctl)        → exit(ctl::run_cli(&argv[2..]))       (ctl.rs:873)
   ├─ Launch::Verb(Mcp)        → exit(ctl::run_mcp_cli(&argv[2..]))   (ctl.rs:1095)
   ├─ Launch::Verb(AgentUsage) → exit(usage::run_cli(&argv[2..]))     (usage.rs:428)
   ├─ Launch::Verb(AgentVitals)→ exit(vitals::run_cli(&argv[2..]))    (vitals.rs:1729)
   ├─ Launch::Verb(Probe)      → exit(probe_cli(&argv[2..]))          (main.rs:18260)
   ├─ Launch::Verb(Serve)      → exit(serve::run_cli(&argv[2..]))     (serve.rs, new; slice-0 stub exits 2 naming the later slice)
   ├─ Launch::Reply{text,code} → println!/eprintln! → exit(code)      (flags via flag_reply main.rs:18320 unchanged; NEW arm: unknown positional → code 2 + USAGE — the #314 kill)
   └─ Launch::Window{open_here}→ alacritty_terminal::tty::setup_env() (main.rs:18408) → scratch/adopt decision → instance::resolve_session (instance.rs:299) → application().run (main.rs:18466); open_here=Some(dir) only for an existing-directory positional (the reserved open-here slot, validated now, threaded as the fresh-session first-pane cwd; a restore ignores it)

HELLO HANDSHAKE (types+doc land slice 0; host/attach domains implement the ends):
serve::run_cli → serve::parse_args (--session required, refuses to default — the #311/#299 cross-wiring class)
→ [host domain] bind $XDG_RUNTIME_DIR/terminal-delight/session-<id>.sock → accept → SO_PEERCRED uid check (auth, nothing else)
→ read one NDJSON line → serde_json::from_str::<proto::Hello>
→ proto::version_check(hello.proto, proto::PROTO_VERSION)
   ├─ Ok  → write HelloReply::Ok{proto, session, attached_gui, build} → verb loop; a second ClientKind::Gui hello STEALS: host drops the previous GUI connection (decision round #1)
   └─ Err → write HelloReply::VersionMismatch{host, client} → close
client (attach domain, slice 3): resolve_session new tier → connect → write Hello{PROTO_VERSION, Gui, build} → read reply → on VersionMismatch: send shutdown verb (host checkpoints via its own persist loop, exits) → today's TOML respawn recovery (new_restored, pane.rs:2455) — degrades to exactly today, once, deliberately (02-architecture:61-63)

SURVIVAL HARNESS (scripts/td-survival-test.sh):
td-survival-test.sh {gui-kill|floor-control|host-kill|flood-stage} [--cycles N]
├─ stage_scene: TD_SESSION=$SID TD_MCP_WRITE=1 $TD_BIN &  → GUI=$! → poll `$TD_BIN ctl --pid $GUI ping`
│  → ctl adopt '{"cwd":"$HOME","run":"seq -f TD-SURV-$RUN-%06g 1 3000"}'   (sentinel pane; 3000 > any viewport, < 10k scrollback)
│  → ctl adopt '{"run":"vim -u NONE /tmp/td-surv-$RUN.txt"}' → ctl adopt '{"run":"htop"}'   (4 panes total — fits the new cap of 4)
│  → ctl mcp on; ctl mcp writes on; ctl mcp expose all
│  → ctl mcp rpc tools/call leave_note {pid:<vim pane>, title, text:"note-$RUN", pin:true}   (mcp.rs:1006)
│  → sleep 35   (past the 30s checkpoint so durable state is on disk before any kill)
├─ snapshot(): ctl mcp rpc list_panes → jq per-pane {pid, tab, mode, cwd, note} (PaneInfo, mcp.rs:87); vim/htop child pids via probe; grep {query:"TD-SURV-$RUN-000001", scrollback:50000} and {query:"TD-SURV-$RUN-003000"}
├─ gui-kill ×N=20: kill -9 $GUI → relaunch same env → poll ping + 4 panes ≤15s → assert vs baseline (shell pids UNCHANGED, vim+htop pids unchanged and kill -0 alive, both sentinel greps hit, tab indices equal, note title/text equal); any miss → losses++, keep cycling
├─ floor-control: same scene, TD_SESSIOND unset (or pre-host build), 1 kill: assert losses==cycles (instrument self-test — a harness reading 0 on today's build is broken) and record the post-TOML-recovery state as control.json
├─ host-kill: TD_SESSIOND=1 scene → pkill -9 -f "serve --session $SID" → kill GUI → relaunch → assert recovered state DEEP-EQUALS control.json field-for-field (tabs, cwds, resume shape, notes; scrollback + vim/htop lost is EXPECTED-EQUAL-TO-FLOOR) — "exactly today's TOML respawn" proven by diff, not argument
└─ summary: one JSON line {leg, cycles, losses, failures[]}; exit 0 iff the leg's expectation holds

LATENCY FLIP GATE (scripts/td-echo-bench.sh):
td-echo-bench.sh → cargo build --release
→ for mode in local attached; for flood in 0 8:
   TD_BIN=target/release/terminal-delight TD_ECHO_MODE=$mode TD_ECHO_FLOOD=$flood cargo test --release echo_latency_bench -- --ignored --nocapture
   └─ term.rs test: measured session = term::spawn_in(cat) (term.rs:105) | term::attach_in vs a spawned `$TD_BIN serve --session bench-$$` [attach domain]; flood=8 adds eight sessions each running `while :; do cat /tmp/td-flood-100mb; done` — 8 panes stays the stress/legacy case per decision round #2/#5
   └─ loop 1000: t0=Instant::now() → session.notifier.notify(b"x") (the exact GUI write path, pane.rs:3436) → spin (50µs sleeps) until term.lock() grid cursor cell shows 'x' (kernel tty echo → EventLoop parse → grid) → push t1−t0
   └─ print {"mode","flood","p50_us","p99_us"} — identical instrument both modes, so the diff isolates the two unix hops
→ jq gate: p99(attached,flood)−p99(local,flood) ≤ 1000µs for flood∈{0,8}
FLIP RULE (decision round #2): both latency gates green AND gui-kill leg green at N=20 AND host-kill leg green → Parker signs the printed numbers; only then does the default flip.

### Test plan

- **known_verbs_dispatch_before_the_gui (main.rs unit, replaces half of main.rs:17172)** — dispatch classifies ctl, mcp, agent-usage, agent-vitals, probe, serve, and --td-emit-demo as Launch::Verb(the right Verb) with is_dir never consulted — serve is a first-class verb, not a positional
- **an_existing_directory_positional_reaches_the_window (main.rs unit)** — dispatch(Some("/home/me/src"), |_| true) == Window{open_here: Some("/home/me/src")} and dispatch(None, ..) == Window{open_here: None} — the reserved open-here slot and the bare launch both still open a window
- **an_unknown_positional_exits_2_instead_of_a_window (main.rs unit — THE #314 pin, inverting the old test's third assertion)** — dispatch on "sevre", "clt", "open-sesame", and "/does/not/exist" (is_dir=false) each yield Reply{code:2} whose text names the offending word and contains Usage — no Launch::Window for any unknown positional
- **version_and_help_are_answered_without_opening_a_window + an_unrecognised_flag_is_refused_rather_than_opening_a_window (main.rs:17151/:17163, KEPT verbatim)** — flag behavior is unchanged by the allowlist rewrite — dispatch routes '-'-leading args through the same flag_reply
- **a_typoed_verb_leaves_no_window_and_no_disk_writes (app/tests/dispatch_cli.rs, e2e)** — the real binary run with ["sevre"] and HOME/XDG_CONFIG_HOME/XDG_RUNTIME_DIR pointed at an empty tempdir exits 2 within seconds, stderr names `sevre`, and the tempdir contains no terminal-delight directory afterward — the phantom-window class loses both its window and its writes
- **serve_without_a_session_id_is_refused (app/tests/dispatch_cli.rs, e2e)** — ["serve"] exits 2 with a message naming --session (distinct from the unknown-verb usage refusal — proves the verb dispatched into serve::run_cli, not the refusal arm)
- **hello_and_reply_round_trip_as_single_ndjson_lines (proto.rs unit)** — Hello and both HelloReply variants serialize to one line each and deserialize back equal — the framing every other TD protocol already uses
- **unknown_hello_fields_are_ignored_within_a_major (proto.rs unit)** — a Hello line carrying an extra unknown field deserializes successfully — additive-within-major is pinned, so new fields never force a version bump
- **a_version_mismatch_names_both_versions_and_build_never_gates (proto.rs unit)** — version_check(2,1) == Err(VersionMismatch{host:1, client:2}); version_check(1,1) == Ok(()); two Hellos differing only in `build` both pass — proto is the sole compatibility fact
- **the_protocol_doc_examples_deserialize (proto.rs conformance, include_str! of docs/protocol/session-host-v1.md)** — every ```json fence in the contract doc parses into Hello or HelloReply — the host is conformance-tested against the DOCUMENT, not the implementation (panel graft)
- **echo_latency_bench (term.rs, #[ignore], via scripts/td-echo-bench.sh)** — prints {mode, flood, p50_us, p99_us} over 1000 echo round-trips through notifier.notify → grid; the script's gate: p99(attached,flood)−p99(local,flood) ≤ 1000µs for flood∈{0,8} — decision round #2's 1ms flip gate, flood held at 8 panes as the stress/legacy case
- **td-survival-test.sh gui-kill --cycles 20 (the metric itself)** — per cycle after kill -9 + relaunch: 4 panes back, every shell pid UNCHANGED, vim pid and htop pid unchanged and alive (kill -0), grep finds sentinel line 000001 AND 003000 at scrollback:50000 (full depth, not tail), PaneInfo.tab indices unchanged, sticky note title/text unchanged; final: losses == 0 over N=20 or exit 1 listing every failure
- **td-survival-test.sh floor-control (instrument self-test + control snapshot)** — the same scene on today's path (TD_SESSIOND unset / pre-host build) reads losses == cycles — a harness that reports 0 on today's build is itself broken — and records the post-TOML-recovery state (tabs, cwds, resume shape, notes) as control.json
- **td-survival-test.sh host-kill (the never-worse gate)** — kill -9 the `serve --session $SID` host mid-run, relaunch: recovered state deep-equals control.json field-for-field — recovery is EXACTLY today's TOML respawn, proven by diff; scrollback and vim/htop lost is asserted as equal-to-floor, never counted as a metric loss, never better required
- **td-survival-test.sh flood-stage (legacy-cap tolerance + eyeball)** — a hand-written LEGACY session TOML holding 8 panes (no pane_id fields, over the new cap of 4) loads with all 8 panes present and running cat floods — the decision-round-#5 loader rule exercised end-to-end; the stage stays up for the no-visible-lag eyeball check while echo_latency_bench supplies the measured number

### Least confident

- The ctl `tabs` op grammar: I verified TabRef::Index/TabRef::Pane exist but not a create-tab op, so the scene builds 4 panes in one tab and asserts PaneInfo.tab indices rather than staging a multi-tab scene; if a tabs op can add a tab, stage_scene should use it and the tab assertion gets stronger
- Whether `ctl adopt` on a window already at the pane cap opens a new tab, splits, or refuses — decides how the 4-pane scene is built under the new cap of 4 (3 adopts onto the 1 boot pane assumes adopt splits the current tab)
- The relaunch environment under Hyprland: the harness assumes `$TD_BIN &` yields the window process as $! (no fork/re-exec) and that a script-relaunched GUI inherits a usable Wayland env; first live run must confirm both or switch to pgrep-by-session and hyprctl dispatch exec
- Echo-bench spin-reads take term.lock() from the test thread and could contend with the EventLoop's FairMutex enough to distort tails; the 50µs sleep spin is a guess — if p99 jitters run-to-run on the LOCAL leg, switch the probe to generation-watch (term.rs:69 Relaxed load) with a single confirming lock
- Sticky-note persistence timing under kill -9: posted notes must be in the durable state by the 30s checkpoint (hence sleep 35) — if notes only persist at quit-save today, the floor-control leg will read note-loss as today's behavior and the host-kill deep-equal still holds, but the gui-kill note assertion post-split then leans on the host's own checkpoint cadence, which the host domain should confirm
- serde tag collision detail: `#[serde(tag = "hello")]` on HelloReply plus struct variants is valid serde, but the exact wire spelling ({"hello":"ok",...}) must be frozen in the doc before the host domain codes against it — the conformance test exists precisely to catch me being wrong here

### Cross-domain needs (resolved by the integrator)

- term domain: `pub fn attach_in(size: GridSize, cell_width: u16, cell_height: u16, stream: UnixStream) -> io::Result<Session>` (beside spawn_in, term.rs:105) returning a Session identical in shape (term.rs:60-74) — the echo bench instrument is mode-blind and reads notifier/term/generation only; until it exists the attached leg skips with a printed notice, it must not fake a number
- host domain: serve::run_cli's body must consume serve::ServeArgs and answer hello EXACTLY per proto.rs / docs/protocol/session-host-v1.md (conformance tests bind it to the doc); SO_PEERCRED uid check at accept and nowhere else; instance::claim_in (instance.rs:471) held for host lifetime; attach snapshot takes FairMutex::lease() then lock() in pty_read's order (vendored alacritty_terminal-0.26.0 sync.rs:28 lease = next-only, :33-36 lock = next-then-data) — the harness's sentinel-under-active-output assertions are the tripwire that fires if this ordering is wrong
- host domain: a second ClientKind::Gui hello STEALS — host drops the previous GUI connection rather than refusing; the gui-kill loop relaunches while the killed GUI's connection may still look half-open, so a refuse-second-attach host deadlocks the harness at cycle 1
- layout/session domain: MAX_PANES (main.rs:75) becomes 4 for new layouts while the loader tolerates a saved layout holding 5-8 panes without dropping a pane — the flood-stage leg seeds a legacy 8-pane TOML and asserts all 8 load; tell me the exact SavedNode/StateFile field set a hand-written legacy TOML needs
- GUI attach domain: on HelloReply::VersionMismatch the client sends the shutdown verb then runs today's TOML respawn (new_restored, pane.rs:2455); the host-kill leg's deep-equal-to-floor assertion is also the acceptance test for that path — wire it so a version skew and a host crash land on the same recovery code
- MCP/ctl surface (unchanged, but load-bearing): the harness instrument rides ctl adopt/mcp rpc (ctl.rs:333 grammar), leave_note (mcp.rs:1006), list_panes PaneInfo {pid, tab, mode, cwd, note} (mcp.rs:87), and grep scrollback:50000 (mcp.rs:654) — any rename or gating change on these breaks the metric instrument and must ping this domain
- instance domain: confirm resolve_session (instance.rs:299) honors $TD_SESSION as tier 1 today — the harness pins its session identity with it; if the env tier differs, the harness falls back to an isolated XDG_CONFIG_HOME sandbox

# Verification record

## Lens: EXISTENCE

- **[WRONG]** encode_snapshot alt-screen leg: 'temporary DOUBLE Term::swap_alt (term/mod.rs:714): with ALT_SCREEN set the destructive branch (:715-724) is skipped and the mode bit does not toggle, so the double flip is observationally neutral'
  - swap_alt exists at term/mod.rs:714 and the destructive branch at :715-724 is real, but the claimed semantics contradict the source. (1) The mode bit ALWAYS toggles: term/mod.rs:732 `self.mode ^= TermMode::ALT_SCREEN` runs unconditionally, outside the if. (2) Therefore the SECOND flip of the double swap runs with ALT_SCREEN cleared, so the destructive branch fires: :717 overwrites the (now-inactive) alt grid's cursor with the primary cursor, :720 clobbers grid.saved_cursor, and :723 `self.inactive_grid.reset_region(..)` ERASES the alt-screen contents the encoder is about to paint. (3) :726-729 also swap keyboard_mode_stack and call set_keyboard_mode on every call, and :733 clears selection on every call. Net: on an alt-screen pane (vim), one encode wipes the live alt screen of the authoritative Term — catastrophically non-neutral, and the design's 'verified against term/mod.rs:714-729 source' claim is false. There is no public accessor to the inactive grid (term/mod.rs:287, private field), so the encoder needs a different route for the primary-grid-under-alt leg (e.g. VT ?1049l/?1049h replay whose alt-clear the replica reproduces identically, or accepting primary-history-only via grid() while on alt). The design's own encode_is_observationally_neutral_and_idempotent test would fail on first run — the pin catches it, but the stated mechanism must be redesigned, and least-confident decision 9's 'verified against source' wording should be corrected.
- **[OK]** Slice-0 dispatch anchors: flag_reply main.rs:18320, main main.rs:18333, if-ladder main.rs:18337-18398, pinning test subcommands_and_bare_arguments_still_reach_the_window main.rs:17173, kept flag tests main.rs:17151/:17163
- **[OK]** Caps: MAX_PANES main.rs:75 (currently 8), enforcement at split() main.rs:4682, badge_overflow test main.rs:16501, MAX_TAB_BADGES compile assert main.rs:18102
- **[OK]** SavedNode main.rs:431 with six existing Leaf fields (appearance/cwd/resume/name/logo/note)
- **[OK]** Persistence chain: persist_primary_state main.rs:1250 (exact signature body:&str,new_leaves,new_tabs,allow_shrink), is_catastrophic_shrink :1189, rotate_state_backup :1207, RefusedShrink stderr :1258, write_atomic session.rs:74, StateFile.panes top-level int main.rs:1067, toml_top_level_usize instance.rs:420, load_state main.rs:1168, richness-guard tests main.rs:17240
- **[OK]** Workspace anchors: Workspace main.rs:1975, build main.rs:2481 (scratch,demo,seed,window,cx), save main.rs:3015 with scratch/released guards :3017-3025, make_pane_restored :2270, build_node :2406, Tab::new :648, load_state :1168, spawn_seeded_window :18236 (current_exe + pre_exec setsid), scratch env check :18422-18444, drop-is-close doctrine main.rs:77-82
- **[OK]** term.rs seam anchors: PtyWrite bounce contract :7-9, GridSize :28, EventProxy generation-bump doc :45-49, tuple struct EventProxy :51 with sole construction site :113, Session :60 (term :61, events :64, master Option<File> :67, shell_pid :68, generation :69-73), resize :85, spawn_in :105, tty::new half :115-141 (vanished-dir filter :123, TD_DEMO :131-138, master try_clone :140), Term+EventLoop half :142-158, headless harness :204-210, feed :218, grid[Line][Column] :224
- **[OK]** session.rs anchors: PaneRestore :27, capture :59, write_atomic :74, agent_resume chain :176
- **[OK]** instance.rs anchors: bind :86, release :98 (disarm-before-unlock comment :99-102), explicit_session :232, resolve_session :299, injectable resolve_session_in :309, rank :378, fresh_session :393, Claim {owned, lock} :459, claim_in :471 (fail-closed comment :472+), TD_SESSION test lesson :1069, config_dir :139
- **[OK]** ctl.rs anchors: TabRef :96 (Index/Pane variants, Copy), ctl_dir :313 (currently private fn — 'was private' accurate), socket_path :321, run_cli :873, owning_td :1031 (/proc parent walk), relay_target :1053, run_mcp_cli :1095
- **[OK]** pane.rs anchors: PaneMode::classify :41-63, foreground_mode :203-217 (tcgetpgrp + /proc comm/cmdline), TerminalView :1786, mirror_document :2312, new_restored :2455, resume-typing recipe :2480-2482, 800ms watcher spawn :2500 gated on session.master :2506, sticky-alt rule :2516-2526, PtyWrite unconditional bounce :2963, send/notify :3436, scroll_display :4159, term.selection write :4238, styled_lines :5027 — and 'all 33 term.lock() sites'
- **[OK]** MCP anchors: ApplyOutcome mcp.rs:318 quoted verbatim as `pub type ApplyOutcome = (Target, Result<GradeReport, String>)`, leave_note mcp.rs:1006, UiReq::Apply mcp_transport.rs:55
- **[OK]** Verb handler anchors: demo::emit_and_block demo.rs:27, ctl::run_cli ctl.rs:873, ctl::run_mcp_cli ctl.rs:1095, usage::run_cli usage.rs:428, vitals::run_cli vitals.rs:1729, probe_cli main.rs:18260
- **[OK]** Vendored tty/mod.rs: Options.env :36, EventedReadWrite :65-78 with register/reregister/deregister taking &Arc<Poller>/Event/PollMode :72-74, ChildEvent::Exited(Option<ExitStatus>) :82-85, EventedPty :92-97
- **[OK]** Vendored event_loop.rs: EventLoop generic :46, new bounds :57-69 (T: tty::EventedPty + event::OnResize + Send + 'static), new :63, pty_read :104, lease held for the whole cycle :117, read :122 before data lock :140 (try_lock_unfair, forced lock_unfair at buffer limit :142), hard-error exit :133, parser.advance :154, pty_write :174, spawn :205, child-event arm :256-271, ChildExit emission :263, Notifier::notify :335 — plus the lease invariant 'every loop exit leaves unprocessed == 0' with the :133 bend
- **[OK]** Vendored sync.rs: FairMutex lease :28 (next-slot only), lock :33-38 (re-takes next at :36), lock_unfair :41-43, try_lock_unfair :46
- **[OK]** Vendored tty/unix.rs: Pty struct :102 (child/file/signals/sig_id :103-106), child() :110, file() :114, tty::new :195, EventedReadWrite impl :323-406, Writer=File :325, register both fds :328-346, writer() :377, EventedPty impl :382, OnResize/TIOCSWINSZ :406 (ioctl :414); event.rs OnResize :98
- **[OK]** SocketPty poll keys ('the fd the Poller watches (key 0)' / eof_pipe '(key 1)') route through the stock run loop's match on event.key
- **[OK]** Encoder read-path anchors: renderable_content().cursor term/mod.rs:637, cursor_style/DECSCUSR :942, TermMode :55, charsets grid/mod.rs:42, history rows Line(-history_size())..bottommost_line() grid/mod.rs:504-517, SGR-relevant Cell fields cell.rs:134-197
- **[OK]** Dependencies: `polling = "3"` addition needed, Cargo.lock already resolves polling 3.11.0 transitively; toml 0.8 assumption; alacritty_terminal 0.26 stock
- **[OK]** Minor cite imprecisions (not load-bearing): scratch_decision test 'main.rs:17095+' and Push::Exit's Option<i32> vs Event::ChildExit(ExitStatus)
- **[OK]** Cross-cutting compile plausibility of proposed signatures (wire_event_loop bounds, fenced_attach/fenced_hash over FairMutex<Term<T>>, ReplicaGuard.term type matching Session.term, Boot::Legacy carrying instance::Claim, TabRef::Durable keeping Copy, HostPane.session: term::Session shared via Arc)

## Lens: COVERAGE

- **[OK]** Binding: metric = lost sessions 0, including scrollback and vim/htop
- **[OK]** Binding: pane/tab close kills (SIGHUP tree), client disconnect kills nothing
- **[OK]** Binding: host crash degrades to today's TOML recovery
- **[OK]** Binding: second attach STEALS, old connection dropped, never a refusal
- **[OK]** Binding: per-window pane cap 4 (MAX_PANES main.rs:75); legacy 5-8 pane layout loads without losing a running pane
- **[OK]** Binding: hosts per-session, setsid, one binary with a serve verb
- **[OK]** Binding: auth = SO_PEERCRED at accept only
- **[OK]** Binding: nothing assumes exactly one attached client
- **[OK]** Binding: selection/scroll stay client state
- **[OK]** Binding: the generation counter stays the invalidation token
- **[OK]** Binding: attach atomicity = FairMutex lease-then-lock in pty_read's order
- **[OK]** Binding: slice 0 (#314 allowlist + main.rs:17172 pinning-test rewrite) independently shippable
- **[OK]** Graft 1: allowlist as its own first slice
- **[OK]** Graft 2: one-page versioned contract doc, conformance-tested against the document
- **[OK]** Graft 3: metric harness built at the opt-in stage, not the flip
- **[OK]** Graft 4: host-kill leg in the harness
- **[OK]** Graft 5: divergence guard (grid hash at generation quiescence, loud re-snapshot)
- **[OK]** Graft 6: schema-versioned opaque layout envelope + TD_PANE_ID ctl resolution before the /proc walk
- **[MISSING]** Graft 7: grid.frame structured-read door recorded and filed as a follow-up
  - The design never mentions grid.frame/grid.lines, the since_gen diff shapes, or filing the follow-up — grep of the DESIGN text finds nothing; it appears in no Files entry, no test, no least-confident item, and there is no follow-ups section at all. 02-architecture.md:134-135 keeps the door open and the panel graft (research-gate2-panel.md:217 and :233) says to file the verb shapes into that issue so the future surface reuses the generation contract. Add a follow-ups line to 03 (or the slice-0/tie-off scope) committing the falsifiable GitHub issue per the house escalate-issue rule; otherwise the graft silently drops between Gate 2 and Gate 4.
- **[MISSING]** Test plan asserts the setsid/detach half of 'hosts are per-session, setsid'
  - No test asserts the host is detached from its spawner: the gui-kill leg uses kill -9, which delivers no SIGHUP, so a host that failed to setsid (pre_exec dropped, or spawned foreground) still passes every listed test and dies later on a real terminal close/logout — the exact loss class the feature exists to prevent. Add an assertion to host_headless_spawn_and_tee_roundtrip or spawn_host's test: the host's /proc sid equals its own pid (or the host survives SIGHUP/exit of the process that spawned it).
- **[MISSING]** Test plan asserts a Cli-kind hello never steals
  - ClientKind::Cli is documented attach-neutral, and resolve_session_hosted tier 2 probes every candidate host with kind:Cli — if a Cli hello ever stole the gui_conn, every window launch and every probe_host call would detach the live window's connection. No test pins it: steal_on_attach tests Gui-vs-Gui only. Add e.g. cli_hello_leaves_the_gui_attached: with a Gui attached, a probe_host/Cli hello + list-panes leaves attached_gui:true and the Gui's streams intact.
- **[MISSING]** Test plan asserts the suppress_pty_write replica contract (DA/DSR answered exactly once; generation still bumps on suppressed events)
  - EventProxy becomes a struct with suppress_pty_write, carrying two load-bearing prose claims — the host owns the bounce so query replies are answered exactly once (host.rs drain_events doc), and generation bumps on every event suppressed or not — but no test in the plan asserts either. A regression doubles every vim DA/DSR reply (the term.rs:7-9 contract) or silently breaks the invalidation token (a binding decision) on the replica. Add a unit: replica EventProxy with suppress=true receives Event::PtyWrite → nothing forwarded, generation advanced; and a host integration sending a DA query through an attached pane asserting one reply on the PTY.
- **[MISSING]** Panel graft outside the listed seven: idle/detached-session backoff for the host's watcher and capture sweeps
  - Both winning-lens graft lists carry it (research-gate2-panel.md:220 and :237 — slow the 800ms mode watcher and capture sweeps when no client is attached, so a fleet of headless hosts is not the new battery drain), but it appears in neither 02-architecture.md nor the design: watcher_loop is specified at a flat 800ms and checkpoint_loop 'runs with zero clients' with no backoff. Since 02 was approved without it, this may be a deliberate drop — but the drop is recorded nowhere. Either add backoff to watcher_loop's contract or record the drop (a follow-up issue), so it does not read as an accidental omission at Gate 4.
