# Native and MCP conformance

These are measured experiments, not a claim that every installed harness version is supported. The exported journal facts are retained as recorded; the qualifications below limit what can be concluded from them.

## Existing Claude, native tracer

`claude-native-verified.json` records the original assignment to the existing Claude process in pane 42. TD acceptance, dispatch intent, missing callback receipt, exact-UUID transcript reconciliation and manager verification are separate events. The output was a 232-byte UTF-8 Markdown file containing one short Lorem ipsum paragraph without a heading. There was one dispatch attempt. A structured recipient MCP result was not collected by this first tracer.

## Chosen loaded Codex conversation

`codex-native-routing-mismatch.json` proves that an already-loaded conversation accepted a native queued message and consumed its exact client-message ID. Its correlated final reply reported pane 3 and declined the assignment. The existing app-server daemon separately exposed a stale TD session and pane 3 in its allowlisted environment.

The helper selected that conversation from the older TD census label for pane 16. It did not establish a declared TUI-to-conversation binding. The new public `agent_targets` resolver explicitly marks that pane unavailable. The helper's recorded binding assertion must not be promoted into kernel or native-client evidence. Pane-specific Codex support requires both a proven TUI/conversation join and correct per-thread tool routing. The [qualification on the existing routing issue](https://github.com/parker-brown-family/terminal-delight/issues/1059#issuecomment-6301664410) preserves this correction. No further Codex assignment was sent.

## Existing Claude, public MCP round trip

`window-mcp-verified.json` records an actual public `agent_send` call, native delivery to the same running Claude process, a recipient `agent_reply` result and a separate manager `agent_reply` verification. The second file contained one 227-byte paragraph. One attempt supplied the native message UUID. An identical-key replay returned the same receipt and attempt count; a changed-payload replay was refused.

The window exposed 23 MCP tools, including all six management operations. Its target list returned 18 declared bindings and eight explicit unavailable entries. A send to an unreachable receiver window failed without adding a local journal event. The Codex manager's capability response was explicitly unsupported: its thread was not loaded on the shared daemon, whose TD routing also differed.

The experimental GUI reused the existing session host and preserved all 30 active saved pane shell PIDs. One additional pane already held as closed disappeared; no new pane process appeared. The host process was unchanged. The default installed-binary symlink, release tags and main branch were not changed.

Private runtime records, including the cutover census, duplicate/conflict responses and forwarding refusal, are under `~/.local/state/terminal-delight/management/experiments/window-mcp/`. Authentication frames, tokens and complete environments are excluded from exports.

## Final validation and window status

Format, production Clippy and workspace tests passed for both terminal cores. The new core has 28 passing tests; the app adds four management MCP/forwarding tests. Main application unit suites passed 2,475 tests for the default core and 2,422 for the fallback, with nine ignored in each. A final locked debug build passed. Existing app test-target lint debt is tracked separately; the management crate passed all-target Clippy.

The experimental window later lost its control path and was replaced by a released window before our restore attempt. The cause and replacement actor were not established. The released executable and the same 30 saved pane processes were verified; this does not establish why the experimental window disappeared.
