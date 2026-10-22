# Program Design: Recorded Relationships and Reliable Handoffs

Status: APPROVED 2026-10-20; Parker: “Concur all - keep rolling”. Product and Architecture are approved. Parker accepts experimental risk and requires records of relationships, models, versions and processes. The handoff semantics below adapt established messaging patterns to the approved local, window-owned design.

## What We Take From Established Messaging

RabbitMQ documents publisher confirms separately from consumer acknowledgements. Its reliability guide also explains why a lost confirmation can lead to duplicate delivery, requiring consumer idempotence. We apply those distinctions rather than claiming the transport makes an agent's work exactly once.

| Established pattern | This design | Deliberate limit |
|---|---|---|
| Publisher confirm | TD confirms only after the request and identity references are durably recorded | Does not establish recipient acceptance |
| Consumer acknowledgement | Correlated input-consumption evidence or the recipient's explicit acceptance of responsibility | Does not establish task success; the acknowledgement's scope is recorded |
| Request/reply correlation | Message, conversation, parent and causation identities; explicit replies refer to their request | Time proximity and matching text are insufficient |
| Consumer credit / prefetch | One handoff awaiting consumption evidence per recipient run; bounded local backlog | Native queue acceptance alone does not release credit |
| Requeue and duplicate handling | Same request key returns the existing receipt; replay into a native adapter requires proven deduplication or proof of nonacceptance | An ambiguous write is quarantined, never blindly repeated |
| Expiry | A bounded local dispatch deadline; terminal disposition is recorded | Passing the deadline cannot retract already-consumed work |
| Dead-letter disposition | Failed, refused, stale and unresolved requests remain in an inspection view with their attempts | Inspection is not an automatically recycled queue |

**Remix verdict: scuttle.** Adopt the documented semantics, mapped into `agent_management/state.rs`, `journal.rs` and `adapters/`; do not copy RabbitMQ code or install a broker. The approved architecture has a single local writer and no broker service. A broker dependency would be a new architecture decision.

Sources: [Confirms and acknowledgements](https://www.rabbitmq.com/docs/confirms), [reliability](https://www.rabbitmq.com/docs/reliability), [request/reply correlation](https://www.rabbitmq.com/tutorials/tutorial-six-python), [prefetch](https://www.rabbitmq.com/docs/consumer-prefetch), [expiry](https://www.rabbitmq.com/docs/ttl), [dead lettering](https://www.rabbitmq.com/docs/dlx). These sources establish messaging behavior; the agent-specific mapping above is our design.

## Identities and Relationships

Use separate identifiers for the continuing conversation, the current process run and each delivery attempt. The word agent alone never keys a delivery record.

```rust
struct ConversationKey { harness: HarnessKind, declared_id: String }
struct ProcessKey { boot_id: String, pid: u32, start_ticks: u64 }
struct RunKey {
    conversation: ConversationKey,
    agent_process: ProcessKey,
    endpoint_generation: EndpointGeneration,
}
struct Location {
    td_session: String,
    pane_id: u64,
    shell: ProcessKey,
}
enum Fact<T> {
    Observed { value: T, evidence: EvidenceRef },
    Unknown { reason: String },
    NotCollected,
}
struct ModelFacts {
    requested_id: Fact<String>,
    reported_id: Fact<String>,
    provider_revision: Fact<String>,
    effort: Fact<String>,
}
struct ActorSnapshot {
    id: SnapshotId,
    conversation: ConversationKey,
    run: RunKey,
    location: Location,
    harness_executable: Fact<ExecutableIdentity>,
    harness_version: Fact<String>,
    inference_runtime: Fact<RuntimeIdentity>,
    model: ModelFacts,
    observed_at: Fact<WallTime>,
}
```

The TUI process, an inference daemon and the model provider are different subjects. `RuntimeIdentity` can name a daemon process/version independently from the TUI executable/version. Unknown provider revision remains unknown even when a family label is available. Each evidence reference names source, observation time and applicable turn or segment when known. Caller-pane attribution is a cooperative local declaration, not authentication; preserve that provenance.

Snapshot both actors at request acceptance, then observe the recipient again at dispatch and acknowledgement. Reuse a snapshot ID only for identical immutable facts. A changed model produces a new snapshot and a `ModelObserved` event; it does not retroactively change an assignment or invalidate a still-correct process destination. A caller can request a model constraint explicitly; an unproven constraint holds delivery rather than silently switching models.

A request creates the relationship. It does not give the sender permanent ownership of the recipient. The same conversation can request work from one agent and receive work from another.

```rust
struct Relationship {
    request: RequestId,
    sender_at_accept: SnapshotId,
    recipient_selected: SnapshotId,
    root_request: Fact<RequestRef>,
    parent_request: Option<RequestRef>,
    caused_by_message: Option<MessageRef>,
}
enum RequestRef {
    Local(RequestId),
    External { instance: String, id: String },
}
```

Validate local ancestry, reject cycles and cap traversed ancestry at 32 relationships per request. A root assignment observes itself as its root; unresolved external ancestry keeps an unknown root. An unknown external parent remains explicitly unresolved; do not manufacture a root. Relationship ancestry never authorizes cascading cancellation, approval or process termination. Moving a conversation to a different process creates a new run; pending requests keep their old run binding. A declared compaction can retain the conversation root; unknown lineage never silently joins it.

The event log is the source of the relationship view. Do not add a graph database or a second ownership registry. `agent_messages` can project request ancestry, actor snapshots and attempts, with the same pagination and exposure rules as its timeline view.

## Envelope and Record Types

```rust
struct SendRequest {
    key: RequestKey,                 // validated UUIDv7, supplied by caller
    target: TargetRef,               // opaque current-run binding from list_panes
    text: String,
    dispatch_before: WallTime,
    parent_request: Option<RequestRef>,
    caused_by_message: Option<MessageRef>,
    expected_model: Option<ModelConstraint>,
    experiment: Option<String>,
}
struct AcceptedRequest {
    id: RequestId,
    key: RequestKey,
    content_sha256: [u8; 32],
    sender: SnapshotId,
    recipient: SnapshotId,
    relationship: Relationship,
    text: String,
    dispatch_before: WallTime,
}
struct Attempt {
    id: AttemptId,
    request: RequestId,
    ordinal: u32,
    recipient_at_dispatch: SnapshotId,
    adapter: AdapterIdentity,
    td_build: String,
    native_request_id: Fact<String>,
}
struct Event {
    schema: ProtocolVersion,
    sequence: u64,
    database_id: String,
    request: RequestId,
    attempt: Option<AttemptId>,
    observed_at: Fact<WallTime>,
    source_time: Fact<WallTime>,
    kind: EventKind,
}
```

The immutable request digest covers the target run, exact UTF-8 text, parent/cause, original dispatch deadline and model constraint using an explicit field-order encoding. Observation metadata is excluded. Same key and different immutable fields conflicts. Retain the bounded original fields for byte comparison alongside the digest; a digest is not a substitute for comparison or evidence. SHA-256 and UUID libraries already appear in the lockfile; declare the existing locked packages directly if required, instead of implementing cryptography.

`AdapterIdentity` records TD's adapter implementation revision, vendor protocol profile, running harness/runtime version observations and advertised capabilities. Every attempt has a new ID; retrying a native call does not become a new request. Credentials, authentication frames, environment dumps and account secrets are never journal fields. Save exact request/reply text privately because it is part of the experiment; persist only allowlisted native evidence.

Events include `Accepted`, `DispatchIntent`, `NativeAccepted`, `NativeHeld`, `RecipientAcknowledged`, `TurnStarted`, `TurnEnded`, `WorkReported`, `CancelRequested`, `CancelConfirmed`, `DefiniteNonacceptance`, `DeliveryUnconfirmed`, `ExpiredBeforeDispatch`, `TargetStale`, `PolicyRevoked`, `ModelObserved` and `ResolutionReported`. Unrecognized journal records are retained but have no dispatch effect; an unsupported major version disables mutations. Malformed records are not silently skipped.

## Receipts and Work Reports

Avoid a single done boolean. A receipt presents independent evidence dimensions, each pointing back to the events that support it.

```rust
struct Receipt {
    request: RequestId,
    confirmed_by_td: EventRef,
    native_delivery: NativeDelivery,
    recipient_ack: Fact<ScopedAcknowledgement>,
    work_reports: Vec<EventRef>,
    resolution_reports: Vec<EventRef>,
    attempts: Vec<AttemptSummary>,
}
enum NativeDelivery {
    Pending { reason: PendingReason },
    Dispatching { attempt: AttemptId },
    Accepted { attempt: AttemptId, evidence: EventRef },
    Held { attempt: AttemptId, evidence: EventRef },
    Unconfirmed { attempt: AttemptId, reason: String },
    Terminal { reason: DeliveryDisposition, evidence: EventRef },
}
enum ReportKind {
    Received, Started, Progress, Question, Result, Failed, Declined,
    Verified, RejectedResult, Abandoned,
}
struct ReportRequest {
    request: RequestId,
    reply_key: RequestKey,
    expected_run: RunKey,
    kind: ReportKind,
    text: String,
    evidence_refs: Vec<EvidenceRef>,
}
```

The recipient run may supply received/started/progress/question/result/failed/declined evidence for itself. A manager may report verified/rejected-result/abandoned, including its own identity and verification evidence. These rules classify the source and scope of evidence; they do not introduce an authorization hierarchy among Parker's local agents. A different manager's verification remains that manager's report and is never relabeled as the original requester's. An acknowledgement says exactly whether it establishes native queue ownership, input consumption or recipient-declared responsibility. Native queue acceptance is not exposed as consumer acknowledgement. Result before acknowledgement is legal; retain the absent acknowledgement. An old duplicate event cannot downgrade stronger evidence. Conflicting native observations remain visible and mark the projected delivery state uncertain.

`Verified` is the reporting manager's assertion with its evidence, not a new claim that TD independently ran its tests. A turn ending and a model saying it succeeded remain observations. Cancelling a request in a journal never pretends to retract already-consumed work. Questions and permission holds cannot be answered by transport acknowledgement.

## State Machine, Credit and Retry

1. Resolve a current target and capability snapshot outside rendering. Accept only directly measured or declared endpoint/conversation bindings; reject inferred-by-elimination or ambiguous ones. Missing model facts are permitted unless constrained.
2. Validate policy, payload, key age, deadline, ancestry and capacity. Look up deduplication before reserving capacity, so a retry can still obtain its original receipt when the queue is full. Reuse the stable sender conversation as the dedupe namespace, not the managing process PID.
3. Insert the accepted request and referenced actor snapshots in one SQLite transaction. Commit durably before returning a publisher-style confirmation. Disk or sync failure returns an error and permits no dispatch.
4. The scheduler retains one unacknowledged handoff per recipient run. Other requests remain in bounded FIFO acceptance order. A held native request or an uncertain write keeps that credit. Native queue acceptance alone does not release it. Correlated consumption evidence or recipient acknowledgement releases delivery credit; this does not mark the task complete.
5. Revalidate target, policy and deadline immediately before persisting `DispatchIntent` and calling the native adapter. Never route by display name, cwd, model label or a saved resume command.
6. A definitely-not-accepted transient failure permits at most three total dispatch attempts while the target remains current and the deadline remains valid: attempt 1 is immediate, followed by delays of 250ms and 1s before attempts 2 and 3. Record each decision and its evidence. Timers do not block GPUI.
7. An ambiguous write records unconfirmed and stops dispatch to that recipient until reconciliation or explicit requester abandonment. Reconcile by the native request ID. Resubmission is allowed only when the adapter's pinned conformance profile proves native deduplication for the same ID; otherwise inspect, do not repeat. Every resubmission counts toward the same three-attempt limit. Abandonment releases local scheduling credit but does not assert the earlier work stopped.
8. Expiry prevents new local dispatch. After native acceptance, attempt native queue cancellation where supported; absent confirmation remains uncertain/too late. The deadline does not promise termination of already-running work. Surface a late native acceptance as late evidence rather than rewriting history.
9. Serialize cancel versus dispatch in the worker. Cancellation before `DispatchIntent` is local and confirmable; afterwards it needs native confirmation. An interrupt requires the exact expected active native turn and its own capability. No fallback kill or launch exists.

The listed retry intervals are policy proposals, not measured latency requirements. Prefer a definite rejection over a retry where it indicates a stale endpoint, denied permission or a malformed request.

## Storage and Experimental Record Keeping

Use the approved per-session management directory with owner-only files. Implement its append-only event journal as `management.sqlite3` through `rusqlite`, already cached locally at 0.37.0; the system SQLite library and headers are installed at 3.53.4. The crate is a new direct project dependency, not a new service. Pin the reviewed version and include it in the repository's license/build checks.

Use SQLite's rollback journal with `journal_mode=DELETE`, `synchronous=EXTRA` and `foreign_keys=ON`, verifying the returned values. A single window-owned state worker owns the connection; the UI reads immutable snapshots. This workload does not require WAL concurrency or a custom log manifest. Set a bounded 250ms database busy timeout; errors stop acceptance or dispatch rather than silently weakening durability. SQLite's atomic commit protects the transaction under its documented filesystem/hardware assumptions; do not claim immunity to storage failure.

| Table | Role and invariant |
|---|---|
| `metadata` | Schema version, database identity and clock high-water mark |
| `actor_snapshots` | Immutable observed facts; snapshot ID and content digest are unique |
| `requests` | Immutable envelope, actor references and parent/cause; unique stable sender conversation + request key |
| `attempts` | One immutable row per attempted native handoff, with adapter/version facts |
| `events` | Ordered append-only evidence; never update historical event content |
| `outbox` | Transactional scheduling projection, rebuilt from requests/events if required |
| `dedupe_tombstones` | Key, content digest and retention boundary after an explicit archival operation |

In one transaction, insert the request and identity snapshots, append acceptance evidence, and make its outgoing work schedulable. Return a publisher-style confirmation only after commit succeeds. Before native I/O, commit dispatch intent and attempt facts in another transaction. Native outcomes and replies append events and update the scheduling projection atomically. Transactions never span a native RPC. The constraints, parameterized queries and foreign keys are part of the implementation's test oracle.

Recover by opening the database and replaying committed events. SQLite rolls back incomplete transactions; TD does not invent its own commit marker or truncate files. Database integrity/schema failure stops mutations and exposes the error and any safely readable evidence. Create exports with SQLite's consistent backup API or a transactionally consistent projection, never a bare copy of a live database. Record archive/prune operations explicitly; historical facts remain immutable.

Sources: [SQLite atomic commit](https://www.sqlite.org/atomiccommit.html), [synchronous settings](https://www.sqlite.org/pragma.html#pragma_synchronous), and [rusqlite](https://github.com/rusqlite/rusqlite). This chooses the implementation of the approved durable journal; it preserves window ownership and introduces no broker daemon.

**Proposed v1 limits:** 32 KiB request text, 64 KiB report text, 128 KiB serialized event, 16 locally pending requests per recipient run, 128 per TD session, one unacknowledged native handoff per recipient, 64 events per read page (maximum 256), a 256 MiB per-session logical record budget, with database/journal size and free disk reported separately. Capacity exhaustion refuses new work; it never drops an active request to accept another. These are bounded starting policies, not workload measurements.

Keep full request, attempt, identity and relationship records until an explicit archive/prune operation. Do not silently auto-delete experimental evidence. Snapshot rows can be reused by content identity. Explicit archival may summarize settled requests only while retaining their evidence references; unresolved attempts remain verbatim. At the budget boundary, ask for archive/prune rather than accepting work without a record. No archive/prune MCP mutation is added in this first feature; an export/runbook belongs with the first reviewed implementation.

Deduplication accepts only UUIDv7 keys within seven days of their creation timestamp and at most five minutes ahead of the local clock. Retain compact key/digest tombstones for at least that horizon, independent of payload archival. An ancient key is refused rather than becoming fresh after pruning. Requests can wait at most 24 hours before their local dispatch deadline. Missing deadline is an invalid request, not an infinite queue lifetime. Clock rollback greater than five minutes from a durable high-water mark pauses dispatch with an explicit clock-uncertain reason. Sequence numbers order observations; wall-clock timestamps never establish cross-process causality.

Each conformance run has an experiment label. Its export includes actor snapshots, TD/harness/adapter revisions, capability evidence, request text and digest, parent/cause links, every dispatch attempt, native receipts, explicit reports, verification references and unresolved facts. It excludes secrets and raw authentication exchanges. A relationship view must answer: who requested this work, which recipient run received it, which models were reported at each stage, which attempts occurred and what established the outcome.

## Files and Interfaces

| Path | Responsibility |
|---|---|
| `app/src/agent_management/mod.rs` | Owned request, identity, evidence, capability and receipt types |
| `app/src/agent_management/state.rs` | Pure reducer, dedupe, credit, deadline and ancestry decisions |
| `app/src/agent_management/journal.rs` | SQLite transactions, event replay, constraints, indexes and bounded export |
| `app/src/agent_management/identity.rs` | Strong live process/endpoint binding and immutable model/runtime snapshots |
| `app/src/agent_management/worker.rs` | Serialized scheduling, bounded commands, timers and immutable UI snapshots |
| `app/src/agent_management/adapters/mod.rs` | Adapter contract and capability/conformance profiles |
| `app/src/agent_management/adapters/claude.rs` | Versioned connect-only peer transport and status normalization |
| `app/src/agent_management/adapters/codex.rs` | Existing loaded-thread queue, turn and cancellation operations |
| `app/src/mcp.rs`, `mcp_transport.rs`, `ctl.rs` | Additive tool schemas, routing, caller/policy handling and worker bridge |
| `app/src/main.rs`, `app/src/pane/bench.rs`, `app/src/workbench.rs` | Start/drop the worker with window ownership; render shared receipt/relationship snapshots |
| `app/src/vitals.rs` | Expose existing raw model observations with provenance, retaining display labels separately |
| `app/src/model_catalog.rs` | Reuse family/version display only when the approved sibling change is integrated |
| `app/src/main.rs` | Register the new module alongside the existing MCP, vitals and workbench modules |
| `app/Cargo.toml`, `app/Cargo.lock` | Add cached rusqlite; direct use of already-locked UUID/SHA-256 packages if needed |
| `docs/spec/td-agent-management.md` | Reviewed wire, retention and evidence contract written alongside the implementation |

The model-identity implementation is currently in `td-agent-model-identity`; it is not present in this checkout. Its `Model` catalog records available and alias-resolved choices. `Vitals::model` is a compact display label, not a raw audit identity. Reuse the underlying transcript parse and expose its raw model field with source information. Do not read a compact chip back into a machine identity or start discovery processes merely to fill missing audit fields. Coordinate the small raw-observation seam before touching that sibling's files.

```rust
trait AgentAdapter {
    fn probe(&mut self, target: &ResolvedTarget) -> ProbeOutcome;
    fn dispatch(&mut self, attempt: &Attempt, request: &AcceptedRequest)
        -> DispatchOutcome;
    fn reconcile(&mut self, attempt: &Attempt) -> ReconcileOutcome;
    fn cancel(&mut self, attempt: &Attempt) -> CancelOutcome;
    fn interrupt(&mut self, run: &RunKey, expected: &NativeTurnId)
        -> InterruptOutcome;
    fn poll(&mut self, budget: ReadBudget) -> Vec<AdapterEvent>;
}
impl Management {
    fn accept(&mut self, caller: &ResolvedCaller, input: SendRequest)
        -> Result<Receipt, ManagementError>;
    fn report(&mut self, caller: &ResolvedCaller, input: ReportRequest)
        -> Result<Receipt, ManagementError>;
    fn cancel(&mut self, caller: &ResolvedCaller, request: RequestId)
        -> Result<Receipt, ManagementError>;
    fn read(&self, caller: &ResolvedCaller, query: MessageQuery)
        -> Result<MessagePage, ManagementError>;
}
fn reduce(state: &mut State, committed: &Event) -> Result<(), ReplayError>;
fn resolve_target(reference: &TargetRef, facts: &LiveFacts)
    -> Result<ResolvedTarget, TargetError>;
```

These are signatures, not implementation bodies. Operation outcomes distinguish definite nonacceptance from ambiguous acceptance, unsupported from unknown, and timeout from known denial. No adapter can request a new agent, raw PTY write or automatic tool approval through this trait.

The single state worker never waits on vendor I/O. It submits adapter calls to a bounded pool of four I/O lanes, permits at most one mutating call per recipient run, and receives correlated outcomes back through its command channel. Proposed connection budget: two seconds; per-call response budget: five seconds. A response timeout remains ambiguous unless the adapter proves nothing was accepted. Bound the state command channel to 256 entries and each native read pass to 64 frames/256 KiB. Full channels return explicit busy/backpressure; callers do not wait indefinitely.

## Call Paths

`agent_send` → caller/policy resolution → bounded worker command → identity/dedupe validation → SQLite transaction commit → receipt response → scheduler → revalidation → durable dispatch intent → native adapter → evidence commit → receipt snapshot + change notification.

Native status or `agent_reply` → validated run/request correlation → allowlisted evidence/report → journal commit → pure reducer → timeline/relationship projection. A callback with the wrong request or run is retained as a diagnostic, never applied to another request.

Window recovery → session ownership → database open + committed event replay → rebuild indexes → observe current runs → reconcile dispatch intents → schedule only definitely-undispatched eligible requests. Existing process trees continue throughout; no host restart or agent launch occurs.

For an explicitly authorized target in another window, the target window owns its request journal and confirmation. Add a typed management-forward envelope to the existing local control transport, carrying destination instance separately from the asserted originating caller and its evidence. Do not reuse the current MCP same-window caller field by pretending the manager sits in the destination pane. The destination revalidates its own write/exposure policy and recipient identity. A forwarding timeout is reconciled using the same key against that destination; it never creates a second local fallback request.

## Test Plan

Every regression check names an observable failure. Test reducer and journal code without GPUI first, then verify the actual public MCP route and both live harness adapters.

1. A sync failure returns no publisher confirmation and makes zero native calls.
2. Dropping the caller's receipt and retrying the same key produces one accepted request and one handoff; changing the payload conflicts.
3. Restarting the managing process in the same conversation preserves its retry namespace.
4. Losing the window after dispatch intent yields unconfirmed/reconciled, never an automatic duplicate write.
5. A replaced process, recycled PID, new conversation, wrong TD instance or changed endpoint generation receives no pending message.
6. Unknown conversation lineage and duplicate holders refuse delivery. Declared compaction preserves the root only when evidenced.
7. A reported model switch produces new snapshots while retaining the old handoff's model facts. Configured and catalog-resolved IDs never become reported identities.
8. A TUI update with an older still-running daemon records both actual versions. Unknown version stays unknown.
9. Two agents in one repo, the same model in two processes and one process used across several requests retain distinct correct relationships.
10. Parent/cause projection preserves fan-out and replies; cycles are rejected and unresolved external ancestry remains unresolved.
11. A consumer acknowledgement releases delivery credit but does not mark work successful. Native queue acceptance alone keeps credit.
12. Held approval, late reply, duplicated acknowledgement, result-before-ack and contradictory native events retain correct evidence without invented completion.
13. Queue capacity refuses new work without evicting existing requests; duplicate reads still return their original receipt when full.
14. Deadline before dispatch prevents the adapter call. Deadline after native acceptance cannot claim cancellation without native confirmation.
15. Cancel racing dispatch has one serialized outcome. An explicit interrupt for the wrong active turn fails without a fallback action.
16. SQLite transaction failure and process death before commit leave no accepted request or outbox work. Database corruption stops mutations. A consistent export includes matching request, event and identity rows; failed archival cannot lose unresolved attempts.
17. Deduplication survives archival; old UUIDv7 keys are refused after the horizon. Clock rollback pauses dispatch instead of extending a hidden lease.
18. Read cursors survive ordinary reconnect; a replaced database or archived coverage gives an explicit reset/coverage response. Policy hides inaccessible targets and stops later dispatch after revocation.
19. Native conformance reaches the already-running Claude, then Codex, in a background pane and preserves a human draft. A shell, missing endpoint, saved-only thread or unsupported authentication results in no terminal input and no new agent.
20. Public MCP sends the original paragraph request, collects correlated evidence and independently reads the resulting Markdown file. Queue insertion or a native method name cannot pass this test.
21. Bounded exported evidence reconstructs sender → request → recipient run → attempt → receipt → reported result with the applicable model facts; it contains no credentials or authentication frames.

Focused tests precede formatting, production compilation/Clippy and the repository's full suites for both default and Alacritty cores, following CI. No application suite is required for this documentation-only turn. Native probes and actual delivery are implementation work after Gate 4 approval, with each experimental outcome retained in the conformance export.

## Least-Confident Decisions

1. Native consumer-consumption evidence may differ between Claude and Codex. A capability remains unknown until its conformance fixture proves the exact scope of acknowledgement; do not substitute queue acceptance.
2. One unacknowledged handoff per recipient is intentionally conservative. Record queue pressure before considering greater credit. Terminal work completion remains a separate question.
3. The record budget and retention policies are starting values. They refuse new work instead of silently losing experimental history; the reviewed export/runbook is required before first use.
4. Claude's private sender/authentication protocol and the existing Codex connect-only handshake remain unverified. Isolate those risks in their adapters; a successful mock adapter does not authorize advertising native support.

Review: `reports/2026-10-20-agent-management-handoffs.html`. Architecture remains approved. This gate settles record types, acknowledgement semantics, bounds and test oracles; Gate 4 will choose the smallest runnable slices.
