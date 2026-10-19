# Git Coordination Program Design

`coordination.rs` contains the immutable observation model, raw-path representation, footprint collection using the existing Git runner, pure grouping/coverage/severity, authorization projection, identity reconciliation and bounded revision history. `coordination_view.rs` owns native inspector rendering and logical navigation through `Workspace`. `engstate.rs` supplies richer observations beside its legacy report. `mcp.rs` registers the four read tools; `mcp_transport.rs` obtains the cached authorized store. `main.rs` gathers pane identity/exposure, applies completed scans and replaces the ticker.

Core records: `Participant`, `PathKey` (display plus exact bytes), `PathChange` (source set and rename links), `Footprint` (four source states, HEAD/base/common-dir, errors and sampling time), `Context`, `Hotspot`, `Cluster`, `Project`, `Snapshot`, `Store`, and `View`. Unknown measurements use options or explicit coverage states. A cluster keeps evidence coverage separate from membership coverage. IDs use physical context identity; lineage is explicit on ambiguous split/merge.

Public flow: `eng_input → engstate::scan → footprint worker → apply_eng → Store::update → native view / authorized MCP snapshot`. MCP handlers query cached data only. Their supplied version/cursor must match the authorized store. Exposure changes clear the authorized cache and history before subsequent reads.

The ten traps to test first:

1. Four checkouts sharing one path produce one hotspot, not six alerts.
2. B/C panes sharing one known session count as one session.
3. A missing footprint joins two groups later; a lower count must not imply improvement.
4. Unknown observations never produce clear or a count of zero.
5. Stacked inherited commits are qualified and cannot become an unqualified conflict claim.
6. Separate clones of one origin do not become one physical checkout or an exact cross-clone comparison.
7. Spaces, tabs, newlines, renames and non-UTF8 path bytes remain distinct and round-trip.
8. Denied-only changes do not move public IDs, revision/cursor state, ordering or deltas.
9. Natural repeated Escape returns to the originating row without a compensating focus call; closed panes are revalidated.
10. A dense common file retains its individual hotspots with bounded output, and an oversized or timed-out command cannot stall the pipe or fabricate complete coverage.

Least-confident calls: conservative treatment of shared authorized/denied storage; qualification of main-based inherited footprints; overlap-based cluster reconciliation; and native width at large chrome scale. Each must have explicit evidence and tests before release. Optional line analysis and push remain unimplemented by design; all four pull/delta tools remain required.

## Implemented refinements

- Scope-local histories never inherit operator deltas when permissions change. Fully exposed native and MCP views share the authorized store; copy-for-agent also stays inside it when the human can inspect more.
- Project inspection accepts an optional checkout ID, paging its source-labelled paths, rename links and conflict evidence even when an isolated alert has no multi-party group. Every checkout summary supplies the exact next read.
- An unresolved index and a committed integration conflict are separate findings. Only the latter binds the selected target and HEAD object IDs. Unavailable index counts remain null.
- Native navigation uses a persistent workspace focus handle and a deferred handoff after tab activation. Separate scroll handles retain each depth/page position; arrow selection scrolls into view. Row content is cached by snapshot epoch/version and view state.
- Unknown pane classification remains an optional agent flag, distinct from a measured shell. Missing observation duration and withheld repository identity remain optional values.
