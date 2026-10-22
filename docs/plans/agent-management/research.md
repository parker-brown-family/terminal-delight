# Existing Seams and Constraints

- `app/src/channel.rs`: `Outbound::Say`, `say_bytes` and `Delivery` encode whole messages and paste/flat/held records. A journal entry does not acknowledge recipient consumption.
- `app/src/pane/bench.rs`: `bench_say` replaces the draft. `bench_deliver`, `bench_channel_open` and `bench_may_write` require visibility, agent mode and no dial picker. Reuse needs a separate management operation preserving draft and recipient identity.
- `docs/spec/td-agent-channel.md`: journal-before-delivery and unconfirmed consumption are explicit. Prompt/reply hooks provide evidence but do not establish correlation to management requests.
- `app/src/mcp_transport.rs`: `UiReq` supports snapshot, appearance, search and document opening, with no delivery operation.
- `app/src/mcp.rs`: tool definitions and dispatch expose no agent communication. Caller metadata is optional.
- `app/src/ctl.rs`: caller identity is asserted local routing metadata, not authenticated process identity.
- `docs/decisions/0002-an-agent-acting-for-you-is-you.md`: preserve accepted cooperative local trust. Outside-user/network callers invalidate its premise.
- `docs/plans/workbench-drives-the-agent/00-status.md`: existing approved channel work. Reuse mechanisms and explicitly review the management-specific visibility amendment.

Architecture must settle host versus window ownership, recipient generation, Claude/Codex acknowledgement, safe readiness and drafts, bounded durable queues, cancellation, replay after reconnect, retention and incremental receipt reads. These are open design questions, not implemented guarantees.

## Architecture Direction

See 02-architecture.md. Native agent connections avoid introducing management commands into the terminal stream. Keep the existing persistent host unchanged and pause the window's delivery worker while it is absent; durable receipt history survives. Adapter conformance, especially existing-process-only connection and uncertainty recovery, remains required before any recipient advertises support.

## Established Messaging and Identity Sources

Official RabbitMQ confirms/reliability/RPC/prefetch/TTL/dead-letter documentation was read for the established handoff semantics. Official SQLite atomic-commit and synchronous-setting documentation supports choosing an embedded transactional event journal. The schema and source mapping are in 03-program-design.md; no RabbitMQ service is installed or proposed under the approved architecture.

Read the existing model-identity sibling's model_catalog.rs and vitals.rs. The catalog contains available/alias-resolved choices and vitals exposes a compact model label. Neither is an audit record of the raw reported model. Program Design proposes exposing raw observations and their provenance through the existing parser, with no sibling source edited in this turn.

## Implementation findings

- The existing Codex daemon control socket speaks WebSocket over Unix sockets. Raw NDJSON and the proxy's stdin were the wrong framing. A direct connect-only handshake and loaded-thread listing succeeded without launching a daemon or an agent.
- Codex's documented socket path is a symlink to a same-user socket in its daemon directory. The adapter pins the canonical endpoint generation and records the daemon process separately from the TUI.
- Claude 2.1.296 accepted the native queued user frame in its existing process. The callback receipt was absent; the exact request UUID in the bound transcript established consumption. Public MCP replies supplied the return path that Claude's peer-name mechanism lacked for this Codex manager.
- The first Codex helper used a pane/session label whose TUI join was not proven. The loaded conversation consumed its exact client-message ID and declined, reporting pane 3. The shared daemon separately carried a stale TD session and pane 3. The current target resolver refuses the unproven join, and the adapter refuses mismatched execution routing. See the qualified evidence in `conformance/README.md` and the existing routing issue.
- The journal is a small GPUI-independent Rust workspace crate so its failure-path and wire tests can run directly. Full immutable records remain retained, so a separate dedupe-tombstone table is unnecessary until an archive/prune feature exists. No pruning API was added.
- Production Clippy passes. Broader Clippy on the app's existing test target still finds previously filed test-only lint debt; the management crate is linted across all its targets. The existing issue is https://github.com/parker-brown-family/terminal-delight/issues/537.
