# Recorded assignments to existing agents

Terminal Delight can assign work to an agent already running in an exposed pane. The recipient may be in a background tab. TD uses the recipient's native connection and leaves the terminal composer alone. It never starts, resumes, forks, restarts or terminates an agent.

The window owns scheduling. Its SQLite journal survives the window; the session host and agent processes have independent lifetimes. Closing the window pauses scheduling. Reopening it validates the journal, rebuilds the outbox from immutable events, and quarantines a dispatch that ended without a confirmed outcome.

## MCP operations

| Operation | Result |
|---|---|
| `agent_targets` | Exact process/conversation bindings, raw model facts and explicit reasons a pane cannot be addressed. Use the returned `run` as the target. |
| `agent_capabilities` | Evidence for that native connection, including unsupported states. |
| `agent_send` | A durable request receipt. Supply `key`, `target`, `text`, `dispatch_before_ms`; optional parent, cause, model constraint and experiment label travel with the request. |
| `agent_messages` | With `request`, a consistent export of its envelope, actors, attempts, events and receipt. Otherwise a cursor page, default 64 events and maximum 256. |
| `agent_reply` | An attributed report: received, started, progress, question, result, failed, declined, verified, rejected_result or abandoned. Supply the request, UUIDv7 reply key, exact expected caller run, text and evidence references. |
| `agent_cancel` | A cancellation record. Cancellation is confirmed only while the request is still in TD's local outbox. |

Each operation accepts an optional `destination: {session, window}` from the receiving window's instance stamp. Forwarding carries the receiver separately from the asserted origin. The receiver checks its own exposure and writes policy and owns the durable confirmation. A failed or incompatible receiver never causes a duplicate request in the source window. Forwarding is one hop and accepts only management operations.

Exposure must be enabled. Sending, replying and cancelling also require the existing writes toggle. Local caller attribution is cooperative, under the accepted same-user trust boundary. A forwarding window is checked against the Unix socket peer; that does not authenticate the claimed original agent. The attribution record preserves this distinction.

Existing clients may cache their tool catalogue. Refresh `tools/list` or reconnect the TD MCP server after updating the window; an agent restart is unnecessary. The ctl transport also accepts standard JSON-RPC tools/call lines under its existing `mcp from <session> <pane> rpc` prefix.

## Receipts describe separate facts

TD confirmation means the request, actor snapshots and outbox entry committed together. Native acceptance means the harness accepted ownership. Input consumption or an explicit recipient report supplies the scoped acknowledgement. A result is a recipient claim; verification is separately attributed to the manager. These facts remain separate in the receipt and Workbench view.

Retry a lost MCP response with the same UUIDv7 key and identical fields. TD returns the existing receipt, including after the recipient disappears. A changed payload conflicts. Keys have a seven-day admission horizon and a five-minute future-clock allowance. Full immutable request records remain retained, so no tombstone or automatic deletion is needed in this version.

TD allows one handoff awaiting consumption evidence per recipient run. Local backlog limits are 16 per run and 128 per window session. Native queue acceptance alone does not release that credit. A manager can explicitly abandon an unresolved handoff; that releases credit without asserting that the recipient stopped working.

A native write with an ambiguous outcome is never automatically repeated. Definite connection nonacceptance allows at most three total attempts, with 250 ms then one-second delays. Native refusal is terminal. A local dispatch deadline is required, within 24 hours. Passing it cannot retract work the recipient already owns.

## Supported native profiles

Claude Code 2.1.296, peer protocol 1: the adapter validates the process, session registry and private native socket. It sends a queued `next` prompt. Exact message UUIDs in the bound recipient transcript can reconcile an absent callback receipt. A mismatched callback is retained as a diagnostic and never applied to another request. The original live test delivered a paragraph assignment to the existing Claude process and independently verified its 232-byte output.

Codex app server 0.163.0: the adapter connects directly to the existing WebSocket over a Unix socket. The conversation must already be loaded. It never bootstraps the daemon or loads a saved thread. The live probe confirmed native queue acceptance and exact client-message consumption in a named loaded conversation. Its association with pane 16 was unproven; the public target resolver marks that pane unavailable. Separately, the native reply and daemon environment exposed stale TD session/pane routing. The adapter now refuses that mismatch before any queue write. See the [shared Codex routing defect](https://github.com/parker-brown-family/terminal-delight/issues/1059#issuecomment-6301664410). Correct per-thread tool routing remains a condition of support.

Other versions, native interruption and cancellation after dispatch are explicitly unsupported. There is no PTY, keystroke or process-launch fallback.

## Records and review

The journal lives at `${XDG_STATE_HOME:-$HOME/.local/state}/terminal-delight/management/<session>/management.sqlite3`. It uses SQLite rollback journals, synchronous EXTRA and foreign keys. A single writer feeds four bounded I/O lanes. Request, report, event and logical-store limits are 32 KiB, 64 KiB, 128 KiB and 256 MiB respectively. A store error stops new mutations; it does not bypass recording.

Actor snapshots distinguish the conversation, agent process, shell, native endpoint generation, inference runtime, requested model, reported model, provider revision and effort. Unknown and uncollected facts remain explicit. Dispatch observes the recipient again rather than changing the original snapshot. A reported model constraint without matching evidence holds the request.

The journal produces a native Workbench relationship diagram and receipt timeline on the requester's pane. They are disposable projections. The journal remains authoritative if publishing the projection fails.

`agent_messages` with a request returns a transactionally consistent projection. Cursor pages support longer histories; verify the database identity and coverage fields. The `Journal::export_to` helper writes a private JSON export with exclusive creation and fsync. Never copy a live SQLite file as an export. Authentication keys, authentication frames and complete process environments are excluded from records. This version has no archive/prune mutation.

## Validation

The small `td-agent-management` workspace crate contains journal, recovery, policy and native protocol tests. The app adds MCP capability and forwarding checks. CI formats and tests the workspace and builds both terminal cores. Live experiment exports under `docs/plans/agent-management/conformance/` preserve the outcome and limits of the actual native probes; they include the separate native tracers and the window-integrated MCP request/result/verification round trip. Both terminal-core workspace suites passed, including 28 management-core tests and four new MCP/forwarding tests.
