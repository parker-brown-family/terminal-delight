# Process authority research

Status: evidence and design questions for Product review, 2026-10-21. This is not an approved architecture or implementation plan.

## User and product boundary

Parker: “Terminal delight really is for a single user, but I don't have a full picture of who would use this and why --- terminal work is LITERALLY ANYTHING”.

One human owner is compatible with different authority for the owner's programs. The unit of restriction should be a launched workload and its children; a pane displays that workload but is not itself a security boundary. A process outside TD can still reach TD's existing same-user endpoints.

Candidate recommendation: retain ordinary Standard terminals and add an optional Restricted launch contract. Reuse an established isolation mechanism rather than create a container engine, agent harness or taxonomy of professions. The command can be arbitrary; the restricted compatibility claim covers only demonstrated profiles and resources. A program needing broader access can be started explicitly as Standard.

## Evidence inspected

Source snapshots: main b85698d50cb32b04bd8f07c9cee4fd908d2e729a and native-handoff head 017da9261e3aaabc73ba8ae0b8a0a8037d175670. The current primary checkout is unrelated WIP on usage-pace-burndown; application files were not edited. These are source observations, not a live exploitation result.

- ctl::parse_line accepts changes to enabled, writable and exposure. ctl::handle_conn queues McpPolicy. ctl::start applies it to ws.mcp and saves the workspace. Caller session/pane are asserted routing metadata.
- ctl::mcp_policy_verbs_queue_and_status_reads_the_mirror expects a writes-off mirror to accept writes-on and all-pane exposure. Inspected, not executed.
- host::serve_connection checks the connection's UID. A byte-stream greeting can attach to a pane; host::keep_typing forwards subsequent bytes to host::write_to. host::handle_control_line admits SpawnPane, ClosePane and other operations under that same UID trust.
- hostproto::ClientKind is explicitly descriptive, not an authorization role. Claiming window or tool does not grant separate powers.
- ctl bench say/submit/choose/click can reach the workbench's input path. The existing terminal-input manifest gap is tracked separately.
- surface::Origin documents that any same-user process can drop a surface file into a pane's directory. File origin is unknown; claims inside the content cannot authenticate its writer.
- agent_management::worker::query checks global writes before sends, replies and cancellation. Caller/origin process facts improve correlation, but a live process reference is not an independently granted authority. Native peer checking verifies the recipient connection, not the original caller's right to instruct it.
- The existing handoff envelope says the message came from another agent, not the person. That is useful attribution, but a language-model instruction to respect the label is not a sandbox.

Relevant records:
- [0002 — An agent acting for you is you](../../decisions/0002-an-agent-acting-for-you-is-you.md) deliberately accepts cooperative local trust. Its stated external-caller expiry condition has not been demonstrated; Product is being asked to reconsider the same-user premise.
- [Control clients granting wider authority](https://github.com/parker-brown-family/terminal-delight/issues/1087).
- [Workbench terminal-write manifest](https://github.com/parker-brown-family/terminal-delight/issues/806).
- [Surface writer identity](https://github.com/parker-brown-family/terminal-delight/issues/483).
- [Existing native-handoff draft](https://github.com/parker-brown-family/terminal-delight/pull/1069).
- [Handoff qualification](https://github.com/parker-brown-family/terminal-delight/issues/1071).

The existing native-handoff status records its approved gates, and the Product and architecture records explain its cooperative-trust scope. Those approvals remain recorded. This proposal does not silently revise them.

## Threat model to approve before implementation

| Situation | Proposed promise |
|---|---|
| Ordinary process in a Standard terminal, acting intentionally or maliciously with host-user access | Retains ordinary account authority. TD cannot claim isolation from it. |
| Prompt-injected agent or compromised child inside a Restricted workload | OS restrictions and mediated TD permissions constrain reachable effects despite the content's intent. |
| Another restricted workload | Separate grants and protected state prevent it using this workload's authority. |
| Host-user malware, administrator or compromised kernel/TD enforcement component | Outside the Restricted containment claim; stronger separation or a VM may be needed. |
| Malicious text or code inside an explicitly granted resource | Can still mislead people or agents. Authorized writes can be destructive. No semantic correctness or harmless-output guarantee. |
| Incoming work sent to a more privileged agent | Never automatically executes using that agent's broader authority under the bounded-handoff claim. |

A Standard process can sabotage a Restricted run because Standard retains host-user rights. The promise protects the host and peers from the Restricted workload; it does not protect the workload from a hostile host. State that direction explicitly.

## Why small fixes alone cannot carry the claim

| Candidate | Useful contribution | Limit |
|---|---|---|
| Document current local trust only | Accurate explanation, no workflow regression | Does not contain a rogue same-user program. |
| Verify peer UID/PID and bind a registered run | Kernel-provided connection evidence and better attribution | Same UID still admits all account processes; a child may share a workload's authority intentionally. PID/ancestry alone is not a durable grant. |
| Add a token in the shared runtime directory or environment | Cooperative protocol separation | A hostile process with the same host-user access may read, copy or replace it; shared files and native endpoints remain bypasses. |
| Remove widening from one friendly CLI | Reduces accidental misuse | Direct socket calls, state edits, host input and automation can bypass it. Server enforcement is required. |
| Ask an AI to judge each request | Can inform a person's review | Semantic judgment cannot replace OS enforcement, and content can be misleading. Not part of the authorization path. |
| Put every terminal in a sandbox by default | Would apply a uniform baseline | Unknown compatibility cost for arbitrary terminal work; unnecessary disruption. |
| Optional Restricted launch plus narrow mediated access | A testable containment claim for selected work | Requires full-path qualification, compatibility limits and protected operator authority. Recommended for Product approval. |

HTTP OAuth guidance concerns HTTP services. This local desktop proposal does not need a login system or enterprise roles merely because MCP is involved.

## Candidate enforcement shape for Architecture to investigate

This is a hypothesis, not a selected implementation:

1. A trusted launch component establishes the workload boundary before executing user-controlled startup files or code. It records the effective policy and its enforcement evidence.
2. The workload receives only its permitted files, processes, environment, connectivity and narrow TD channel. It never receives the host/window control sockets, management state, operator credential, shared native-agent daemon or unfiltered desktop bus.
3. A TD gateway authorizes each operation against an independently issued grant. Claimed session, title, directory, peer name or model cannot mint authority. Content can request access; it cannot activate that request.
4. The operator channel is protected from the workload's filesystem, process and desktop-automation access. Standard scripts may retain operator automation, but that channel is outside the restricted workload's reach.
5. A grant binds to a workload run, explicit resource/recipient and allowed effects. Restart, expiry and revocation have defined behavior; a replacement run cannot inherit authority just by reusing a label.
6. A child inherits at most its workload's approved scope. This first contract does not pretend to distinguish a helpful dependency from a malicious one inside the same workload.
7. Direct and alternate routes must be tested. A gateway cannot make a promise while a raw host or native endpoint is still reachable.

Keep protocol preferences and enforced security limits separate. Requiring a human click in one screen is insufficient if that same workload can synthesize the click or modify the saved grant.

### Isolation candidates and sources

[Bubblewrap's own documentation](https://github.com/containers/bubblewrap) says it is a low-level tool, with the calling application's policy determining protection. It warns about exposing the desktop bus and terminal input injection. Candidate: reuse the installed executable behind TD's launch contract; no fork or copied code. Source verdict: scuttle its boundary lessons, reuse its mechanism if it passes Architecture's tests. Do not adopt a generic example as a security policy.

[Linux Landlock documentation](https://docs.kernel.org/userspace-api/landlock.html) describes restrictions on the calling thread and its future children, with versioned filesystem, network and scope facilities. Candidate additional enforcement; supported kernel ABI and specific denied routes must be measured. Presence of a binary or a recent kernel is not qualification.

[Flatpak permissions documentation](https://docs.flatpak.org/en/latest/sandbox-permissions.html) demonstrates scoped access and mediated resource selection. It cautions that full bus access removes filtering. Source verdict: scuttle its resource-grant interaction and service-access cautions; do not convert TD into Flatpak.

[Linux Unix-socket documentation](https://man7.org/linux/man-pages/man7/unix.7.html) defines SO_PEERCRED as credentials at connection establishment. It does not authenticate a natural-language statement about which pane or human sent a task.

[systemd execution documentation](https://www.freedesktop.org/software/systemd/man/258/systemd.exec.html) is a second mechanism candidate. A namespaced UID or ProtectProc setting alone must not be mistaken for a complete boundary from same-user processes.

Local read-only survey: bubblewrap 0.12.0 is installed; kernel 7.2.5-3-omarchy; ptrace_scope=1; legacy_tiocsti=0. No sandbox was launched and Landlock support was not probed. These facts establish available research inputs only.

### Connectivity and credentials are a blocking design question

An agent normally needs its provider. A restricted shell normally may not need any network. Full host networking can expose loopback services; a broad desktop or native-harness socket can bypass the file boundary. The design must choose and prove a network/credential policy, rather than advertise “internet only” from a filesystem sandbox.

Possible provider routing and credential mediation are architecture questions, not implemented products. If a native harness must retain broad secrets or a shared privileged daemon, that profile remains unavailable for the strict claim. Existing native sandboxing may be reused only after its coverage of TD access, external tools and native endpoints is measured.

No estimate is made for universal compatibility, implementation duration or latency.

## Delegation without authority amplification

A request from a restricted sender to an already-running restricted recipient can be authorized as a route with a bounded purpose and effects. The recipient must have an independently enforced ceiling suitable for that work. A sender having “send message” permission must not imply permission to invoke all tools the recipient owns.

A plain-language task inserted into an existing full-access conversation cannot mechanically narrow that conversation's tools. Recording origin is insufficient. Until the harness and enforcement path can prove a lower execution scope, bounded work to a Standard recipient is a proposal for the person, not automatic execution.

Operator authorization may deliberately delegate a capability the sender cannot exercise directly—for example, a specific write through a tightly limited reviewer service. Represent that as an explicit delegation grant. Do not compute a naive intersection of unrelated sender/recipient file permissions and call it sufficient.

The existing-agent-only rule remains: management does not launch, resume, fork or replace agents. A person can manually start a restricted terminal or agent as a separate launch feature. Stronger per-task execution isolation is unavailable until proven by a supported native profile; the plan does not assume it exists.

Unknown origin stays unknown. A forwarding hop cannot promote cooperative attribution to verified operator authority.

## Revocation limits

Broker revocation blocks new authorized operations and pending undispatched work once applied. It cannot retract already-read secrets, network messages or completed writes, and may not stop a task already running in a recipient. Stopping a workload is a separate human action; supporting stop must not accidentally imply terminating an unrelated process.

Closing a pane, closing a window and ending a workload are different events. Architecture must decide durable enforcement ownership across a window restart. The initial safe grant policy can revoke and require a new grant after enforcement-owner recovery; saved recipes are suggestions, not silently restored authority.

Shared writable work directories are an explicit influence channel. If another Standard agent automatically executes their contents, it can amplify authority even when no message crosses TD. A Restricted label cannot promise protection from downstream human or program execution of its authorized outputs.

## Adversarial corpus to finalize at Program Design

All outcomes are currently unmeasured. Tests run only in isolated scratch sessions with canaries, never against Parker's live work.

1. Direct policy changes: enabled, writes and exposure cannot be enlarged by the restricted workload, even when bypassing the CLI. A workload must not revoke or disable unrelated workloads' global access either. The current narrow PR intentionally permits unauthenticated reductions; deciding that denial-of-service behavior and its audit trail belongs to the complete authority design.
2. Claimed identity: forged pane/session, forwarded origin, peer name and PID reuse grant no additional authority.
3. Raw host: attach, terminal bytes, spawn, close, checkpoint and window registration cannot bypass the workload's channel.
4. Human surface: workbench say/submit/choose/click, document notes, synthetic input and surface files cannot manufacture operator consent.
5. State and credentials: reads/writes through config, grants, session state, native endpoints, transcripts, inherited descriptors and environment cannot reach protected canaries.
6. Process escape: parent/sibling process access, signals, process descriptors and daemon reparenting cannot escape the launch boundary.
7. Services: desktop bus, compositor, X11, SSH agent, container engine and native harness endpoints are blocked or explicitly scoped.
8. Network: granted external access does not accidentally admit protected loopback, host services, alternative address families or unrestricted proxies.
9. Files: traversal, symlinks, hard links, rename races and shared writable repository metadata cannot widen the documented resource scope.
10. Grant lifecycle: expiry, revocation, reconnect, duplicate delivery, broker restart and recipient replacement never resurrect old grants. In-flight work is reported separately.
11. Deputy: a permitted message cannot automatically invoke a recipient's ungranted stronger effects. Re-forwarding does not reset provenance or limits.
12. Content influence: malicious tool output, surface actions, hooks, document rendering and a later privileged consumer are tested and classified. A trusted renderer opening restricted HTML must not fetch protected host resources or perform ungranted network/actions on its behalf. This is a required test, not a verified renderer defect. Authorized output is not deemed semantically safe.
13. Availability: bounded input, output, request queues and pending approval proposals prevent one workload exhausting TD's control path. Resource-limit coverage is explicit; no total DoS guarantee.
14. Compatibility and positive controls: a normal shell/TUI and representative local build and agent work complete allowed actions without per-call prompts. Failed/unsupported profiles never start silently as Standard.

For every negative test, include a positive control showing the operation succeeds when deliberately authorized, and a receipt that it reached the expected boundary. An unrelated routing failure is not a security pass.

## Proposed investigation order

1. Inventory operations and assumptions; reproduce existing authority in a scratch instance. Preserve the existing cooperative behavior's tests as a named Standard contract.
2. Prove one manually launched Restricted shell with no TD rights. Block alternate routes and verify host-canary secrecy and integrity. This is the first feasibility experiment.
3. Add one own-pane publication operation through independently granted access. Prove that the same client cannot reach another pane, issue a grant or reuse revoked authority.
4. Prove one existing-recipient handoff with a constrained effect ceiling; attempt the deputy and shared-native-endpoint bypasses.
5. Expand one observed compatibility need at a time. Refuse unsupported combinations and preserve Standard for arbitrary work.

This is a roadmap for the next gates, not an approved implementation slice list. Stop or reduce the restricted-mode promise if the first experiment cannot establish it with existing isolation mechanisms; do not build a new container/harness platform to salvage the claim.

The November 20 target is not evidence that all of this fits the next release. Baseline hardening can ship with an accurate shared-trust statement; a Restricted claim waits for demonstrated isolation. No current release schedule or accepted feature approval was changed by this document.
