# Status: Managing agents through Terminal Delight MCP

**Difficulty: 8/10** — a communication contract orchestrators will rely on; wrong-target delivery and ambiguous retries can cause work after a caller reports success. Raised from 5 after Parker clarified the management role.
**Turned out to be: 8/10.** Native framing, uncertain delivery and independent TUI/runtime identity all mattered. The first Codex probe also required an explicit qualification of its unproven pane-to-thread join.

- Gate 1 — Product: APPROVED 2026-10-20 through both concur stamps. Parker clarified: existing agent processes only; never launch a new agent.
- Gate 2 — Architecture: APPROVED 2026-10-20 through both concur stamps; existing native connections and window-owned durable scheduling.
- Gate 3 — Program Design: APPROVED 2026-10-20; Parker: “COncur all - keep rolling”.
- Gate 4 — Vertical Slices: implemented; native Claude and public MCP request/result/verification round trips measured. Codex remains explicitly unavailable where identity or tool routing is unproven.

## User intent

“Think hard as this will be HOW an orchestrator / managing agent talks and manages other agetns...”

The trigger: prompt the Claude agent in the task 1 tab to create one Markdown file with one short Lorem ipsum paragraph. A sticky note did not satisfy it.

## Established evidence

A fresh MCP connection returned 17 tools and no prompt-delivery verb. The existing channel journals messages but gates delivery on pane visibility. Its record does not confirm recipient consumption. The accepted local trust decision permits agents acting for Parker to act as him.

## Brief Verification

Chromium at 390px and 968px: no horizontal overflow; all five figures and both decision questions are annotatable; no external resources. The narrative gate classified the page as a journey, cold-reader opening 0.75 and heading continuity 0.89. It flagged the transition into recipient binding; the figure caption was revised to lead into retry behavior. These are brief checks, not implementation tests.

## Current implementation

Source lives in the isolated `/home/parker/Work/td-agent-management` checkout on `feat/agent-management`. Six public MCP operations use an embedded SQLite event journal, transactional outbox, immutable actor snapshots and bounded native I/O lanes. The Workbench relationship diagram and receipt timeline are projections of those records. Existing-agent-only scope and the unchanged host boundary are preserved.

The original native tracer produced and verified a 232-byte paragraph. A live public `agent_send` → recipient `agent_reply` → manager verification round trip produced and verified a 227-byte paragraph, with one attempt. Identical-key replay returned the same receipt; changed payload conflicted; unreachable forwarding left the local journal unchanged. The current capability path refuses unproven Codex identity/routing. See `conformance/README.md` for the scope of each experiment.

The experimental session-1 window reused the existing host and all 30 active saved pane PIDs. Default installation, release tags and main were not changed. After the experimental window was replaced by a released window, the same 30 saved pane processes were verified again; the cause and replacement actor remain unproven. Release coordination identified this as an independently reviewed RC.2 feature; no RC.1 merge is intended.

## Resume

Final format, production Clippy, workspace tests for both terminal cores and the locked debug build passed. The implementation review page is complete. Prepare one draft RC.2 PR for review. Product, Architecture and Program Design remain approved. Do not redo them. Preserve the independent request/native-consumption/work-result/verification dimensions and the explicit Codex unsupported states. Existing workbench-channel approvals remain settled; management uses its approved native route for background delivery.

## Product Approval

Both product questions received CONCUR. Parker: “This does NOT mean starting a new agent... this means an agent process already runing in another TERMINAL DELIGHT tab / pane / process.” New turns in the existing conversation are permitted; new agent processes, forks, automatic resume and replacement launches are outside this feature.

## Architecture Review Evidence

- Opened existing host, identity, tenancy, channel and MCP boundaries; inspected installed Codex/Claude help and native protocol surfaces.
- Codex CLI 0.162.1, running daemon 0.163.0. Generated schemas from the running daemon binary expose queue add/list/delete/start with client message identity. Native deduplication remains unproven.
- Claude 2.1.296 peer handling exposes correlated held/denied/expired/delivered/refused/dropped outcomes. Authentication and TD sender registration remain conformance questions.
- Two read-only Codex direct/proxy attempts did not complete initialization. No agent was started and no message sent. Resolve the connect-only route in the adapter conformance work; do not infer that help text is a delivery proof.
- Architecture brief: Chromium 968px and a true 390px iframe viewport both show no page overflow. Six of six figures and two of two decision questions accept notes. All three detail-dialog references resolve, with no external resources.
- Revised narrative gate: cold-reader opening 0.37 to 0.81, heading story 0.89, arc argument. Replaced the remaining task-label heading after that run. These are document checks, not adapter tests.

## Architecture Approval and Program-Design Steering

Parker concurred with both architecture questions. He accepts experimental risk and requests good record keeping, particularly the relationship between agents, their models, versions and processes. He points to RabbitMQ and similar services as established sources for handoff, acknowledgement and management semantics. Preserve the approved architecture; apply those patterns rather than introducing an unapproved broker service.

## Program-Design Review

Program Design records continuing conversation identity, exact process run, immutable model/version observations and per-attempt adapter/native/TD evidence. Relationships derive from request parent/cause links rather than permanent agent ownership. It maps RabbitMQ confirms, consumer acknowledgements, correlation, credit, retries, expiry and inspection dispositions to the approved local design.

The append-only journal is proposed as an embedded SQLite event store with a transactional outbox. Local read-only inspection verified SQLite 3.53.4 and headers, cached rusqlite 0.37.0, and existing locked SHA-256/UUID dependencies. SQLite is a new direct crate dependency, not a broker daemon. No implementation dependency or source files were changed.

The review page has six figures. Chromium at 968px and a true 390px iframe viewport: no horizontal overflow; six of six figures and two of two decisions annotatable; all three modal references resolve; no external assets. Visual capture inspected. Narrative gate: journey, opening comprehension 0.81, heading story 0.59. These are document checks; the 21 planned checks and native delivery have not run.

## Implementation review verification

The five-figure implementation page is self-contained. Chromium at 968px and 390px found no horizontal overflow; all figures and summary cards are annotatable, all modal references resolve, and no external assets are required. The narrative check classified it as an argument with heading continuity 0.77; it still rated a cold stranger’s opening comprehension low (0.19). Plain-language opening and captions were revised; the page remains an implementation review for the reader who approved the design, rather than a public introduction.
