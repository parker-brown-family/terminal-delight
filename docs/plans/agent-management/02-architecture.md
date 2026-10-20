# Architecture: Direct Existing Agents Through Their Native Connections

Status: APPROVED 2026-10-20 through both architecture concur stamps. Product approval includes Parker's clarification that the recipient is an agent process already running in another TD pane.

## Fit

TD provides one management contract over the existing processes' own input channels. It owns target selection, durable receipts, bounded delivery scheduling and reply correlation. The harness owns its editor, pending approvals and execution. The persistent terminal host continues owning the PTYs; this feature neither upgrades it nor uses its raw input stream.

The first implementation is window-scoped, using the existing session ownership lock. A background worker services a bounded journal outside the GPUI thread. Losing the window pauses new dispatch; the agent processes keep running, and a replacement window replays receipts before dispatching anything. This deliberately avoids a new daemon and a live host restart. Cross-window management routes explicitly through the target window's existing local control endpoint and policy; it never guesses a window or broadens exposure.

## Recipient Binding

Extend the existing pane census with an opaque target reference and per-operation capabilities. The reference binds the TD instance/session, pane ID, shell PID/start time, current agent PID/start time, exact conversation identity and native endpoint generation. Titles and directories remain labels. Missing or ambiguous identity produces an unavailable target, not a guessed one.

Reuse `paneident`, `session`, `tenancy` and `/proc` observations, but do not promote every identity acceptable for reading into authority to write. A management target needs a declared or directly measured association to the native endpoint. Duplicate conversation holders or uncertain lineage refuse delivery. A declared compaction in the same conversation/process can preserve the binding; `/clear`, different conversation, restarted process or replacement socket requires a new target. Unknown lineage does not silently inherit pending requests.

The target worker revalidates before every native dispatch. Input goes to the native endpoint, never to a terminal. An endpoint disappearing cannot cause bytes to run in the shell. A harness that could load a saved thread is invoked only through an operation proven to target the already-loaded instance; no start, resume, fork, daemon bootstrap or replacement fallback is allowed.

## Public MCP Operations

- `list_panes`: add target reference, capability states and evidence timestamps. A capability is supported, unsupported or unknown, with a reason. Preserve existing fields and exposure policy.
- `agent_send`: target reference, caller-supplied request key, text, optional correlation/parent request, and a bounded expiry. Default behavior is a new assignment after current work, not interruption. Return an accepted request receipt only after durable storage.
- `agent_messages`: bounded cursor read of a request conversation or the caller's inbox. Carries the latest receipt and ordered evidence events. Reconnect with the cursor; history expiration returns an explicit reset requirement. There is no long-running MCP call waiting for a model response.
- `agent_reply`: recipient explicitly reports acknowledgement, progress, a question or a result against a request. Bound to the recipient run; persisted as agent-reported evidence. A question does not approve tools or permissions. This writes a response record, not an unsolicited new task in the manager's prompt.
- `agent_cancel`: cancel before dispatch, or request native queue removal where supported. Distinguish confirmed cancellation, too late, unsupported and uncertain. Never report cancellation from request acceptance alone.
- `agent_interrupt`: separate operation, capability-gated against an expected active turn. No kill, restart or implicit escalation. Its implementation belongs after basic communication conformance.

MCP change notifications announce new evidence where supported. Cursor reads remain authoritative. Notifications do not turn every status update into a prompt; this avoids self-sustaining agent reply loops. A managing agent chooses when a result deserves another assignment.

## Adapters

### Codex

Connect to the already-running local app-server through its supported control/proxy route, negotiate the running protocol and resolve the exact loaded thread behind the selected TUI. Reject unloaded/saved-only threads. The installed daemon is 0.163.0 while the shell CLI is 0.162.1: negotiate rather than assume their schemas match.

The running daemon binary's generated experimental schema exposes `thread/queue/add`, `list`, `delete` and `start`, including a client user-message ID. Use those for queue semantics only after proving that add and start cannot load a missing thread or restart a conversation. `turn/steer` and `turn/interrupt` are separate capabilities with expected-turn preconditions. Never pass model, cwd, approval or sandbox overrides as a side effect of management input.

Native acceptance, queued submission identity, user-message/turn events and explicit recipient replies are separate evidence. A field named client user-message ID does not by itself prove native deduplication. Reconcile a timed-out submission by native queue/history identity; if the native state cannot establish the outcome, stop at unconfirmed instead of retrying.

### Claude

Connect to the exact process-owned peer socket already used by cross-session messages. Resolve it from the selected live process, not a peer display name. Respect its current authentication and namespace rules. Any adapter registration identifies TD as the sender and cannot masquerade as a human or another Claude session.

The installed Claude 2.1.296 peer implementation exposes request correlation and held, denied, expired, delivered, refused and dropped status frames. Map those without reinterpreting delivered as model comprehension. Approval holds remain holds. Native idle notices are readiness evidence, not proof that a requested task succeeded.

This is a private vendor protocol, so isolate it in a versioned adapter and pin conformance fixtures to the observed version. We inspected the installed artifact; no vendor implementation is copied. Unsupported authentication or reply-address requirements return a capability reason. Do not synthesize an approval or fall back to PTY input.

### Admission Rule

An adapter is advertised as supported only after it passes the real harness conformance checks: existing-process delivery, background operation, preserved draft, approval hold, recipient replacement, correlated acknowledgement, cancellation and uncertain-write recovery. A CLI help entry or a generated schema is evidence of an interface, not a passed delivery test. Unsupported cancellation or interruption does not disable an otherwise proven send capability.

## Data and Recovery

Store per-session management history under `$XDG_STATE_HOME/terminal-delight/management/<session>/`, outside the surface mailbox. The window elected by existing session ownership is the single writer. Use an append-only, revisioned journal, with bounded request text, active queue, retained receipt history and explicit capacity errors. A worker performs file and socket I/O; the UI receives immutable snapshots.

Persist acceptance before returning its receipt; persist a dispatch-intent event before the native call. Include sender, recipient binding, request key, content digest, parent correlation, expiry and local sequence. Append native observations and replies; do not overwrite absent fields with false, zero or empty text. The native adapter's evidence time and TD's observation time remain distinct.

The key is scoped to the stable sender conversation and target, so restarting the managing process does not create a second assignment. Sender process identity remains provenance, not the retry namespace. Require an exact stable sender identity for mutation. Same key and same payload returns the original receipt, including any terminal state. Same key with different payload is a conflict. A new assignment always needs a new key. Keys carry a validated creation epoch; requests older than the declared deduplication horizon are refused, never reaccepted as fresh after compaction. Retain compact tombstones through that horizon. Program Design fixes the key format, clock policy, horizon and capacities before code.

On recovery, definitely-undispatched work can continue only after policy, expiry and recipient identity checks. A dispatch-intent without a native receipt is uncertain. Reconcile through the adapter's own ID/status if supported; otherwise leave it unconfirmed. Neither a timeout nor window restart triggers blind resubmission. Host restart, recipient process replacement or unknown lineage invalidates old pending destinations.

Cancelling and dispatching share the worker's serialized state transition. Once a native call may have taken effect, cancellation needs native confirmation; it cannot be a local flag pretending to retract input. Policy revocation prevents further dispatch, while retaining receipts.

## Observation, Authority and Completion

Preserve the accepted same-user local trust decision and existing exposure/write policy. Caller fields name a cooperative local sender; they are not authenticated proof of pane ancestry. No per-message confirmation is added. Remote users and network access would require revisiting that decision.

Management input carries a distinct origin and request identity. The recipient receives a request from a manager, not a forged human/workbench instruction. It remains subject to the user's scope and its own harness controls. Foreign panes' transcripts and outputs remain untrusted observations.

Delivery status and work status are separate typed dimensions. A native receipt can establish accepted/held/delivered; only correlated input evidence or an explicit acknowledgement establishes receipt by the recipient. Work reports identify their author and request. A turn-completed event records the turn outcome, not task success. The managing agent independently verifies returned artifacts or test evidence before deciding the assignment is complete.

## Existing Components and Changes

- `mcp.rs` and `mcp_transport.rs`: additive tools, structured target/capability metadata and a bounded asynchronous worker bridge.
- `ctl.rs`: explicit target-window routing through the existing local endpoint; preserve policy and same-user trust. No raw-input control verb.
- `paneident.rs`, `session.rs`, `tenancy.rs`: reuse direct process/conversation evidence; management can reject weaker bonds.
- New `agent_management` module: pure request state machine, journal and scheduler, separate from workbench editing. New adapter submodules own vendor wire details.
- Existing `channel.rs`, `transcript.rs`, `surfacefeed.rs`: reuse event vocabulary and readers where semantics match. Do not store management commands as workbench surfaces or treat surface files as delivery authority.
- Workbench: render a request's delivery evidence and reported work evidence. Preserve the existing composer and human stop controls. The manager MCP and workbench read the same receipts.
- `host.rs`, `hostproto.rs`, existing running agent commands: no changes required by the selected delivery route.

## Least-Confident Decisions

1. Both native paths must prove connect-only behavior against the already-running process. Codex schema inspection succeeded, but the initial direct and proxy read probes did not complete a handshake. Claude's required sender registration/authentication must be verified without impersonation. These are conformance gates, not reasons to type into a PTY.
2. Queue ordering relative to human input depends on the harness. Preserve its order and report evidence; do not promise a total order across human and manager inputs before measuring it.
3. Window-scoped scheduling pauses delivery while the window is absent. Durable receipts survive. Moving this to a permanent broker is a future decision only if unattended delivery without a window becomes required.

## Evidence

Read on 2026-10-20: the TD components cited above; installed `codex --help`, `codex queue --help`, daemon version, schemas generated from both CLI and running-daemon binary; installed Claude peer status handling. No message was sent and no agent was started during this research.

Official OpenAI documentation describes loaded-thread discovery, queued/steering input and turn events: https://learn.chatgpt.com/docs/app-server. Local generated schemas establish the installed queue methods; the web page alone does not establish that this daemon accepts them.

The architecture review is `reports/2026-10-20-agent-management-architecture.html`. Approval selects these boundaries; Program Design must make the remaining limits and adapter checks executable before implementation begins.
