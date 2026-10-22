# Program design: fail-closed launch and per-run grants

`restricted.rs` owns CLI parsing, work-root validation/pinning, private run-directory lifecycle, sandbox argv construction, filtered I/O, gateway and operator loops. Main dispatch names the verb before GUI initialization. Existing ctl helpers own pane resolution and MCP note forwarding. No host wire or persisted session schema changes.

CLI: `restricted run --workdir PATH [--read-only] [--td-note] -- PROGRAM ARGS...`; `restricted status RUN`; `restricted revoke RUN`; `restricted dismiss RUN`; inside, `restricted call JSON`. Run IDs are random 128-bit values with exclusive directory creation. The operator path is derived only from a validated ID. Direct callers outside the sandbox have ordinary Standard account authority; protection runs from the restricted workload toward the host.

Gateway tagged operations: `status`, `note` with text only, and `request_access` with reason only. Unknown operations and fields are refused. No target, generic tool name, pin, file path, script, approval answer or delegation field is accepted. Text is bounded and control-free. The gateway exposes no document-rendering or surface-action deputy. Status carries selected scope, revocation and pending proposal; unavailable capabilities are named. The supervisor prints one lifetime proposal with the current resource, workload-supplied reason and new-run consequence. Outside-only dismissal clears it without changing grants or resetting its budget.

The note grant is immutable and scoped to one captured session/pane/shell start tick. Revoke holds the same lock as dispatch, so its acknowledgement follows any accepted in-flight note. A replacement process/pane or window not serving the captured session is refused. Existing global writes/exposure controls may still refuse a granted note. The broker never widens them.

Child standard streams are pipes; the supervisor marks all non-stdio descriptors close-on-exec, preserves only the work/filter setup descriptors consumed by bubblewrap and the preconnected TD datagram descriptor, and bubblewrap detaches the terminal session. A seccomp filter denies new connections and addressed sends, including io_uring and alternate syscall ABIs; this also blocks a host socket added to the shared work tree after validation. Streaming UTF-8 filtering preserves split characters, repairs invalid bytes and strips terminal controls. Memory is bounded per stream/socket request; request loops have timeouts and rate limits. Complete CPU/memory quotas are not claimed. Supervisor death kills the sandbox; completion cleans children and removes ephemeral sockets and records.

Regression oracles exercise real socket parsing, work-directory refusal, plain-text output, sandbox canaries and positive allowed work. GUI pointer coverage uses the deterministic GPUI harness plus a routing source gate; removing the check must fail. E2E uses scratch directories, a socket service, loopback listener and scratch native window, never live panes.

## Evidence-driven changes during qualification

The first OS test refused the parent-/proc directory mount under ptrace policy; direct --bind-fd now pins and verifies the directory. The next test exposed a real inherited canary FD: bubblewrap preserves unrelated inherited descriptors, so the supervisor must explicitly mark them close-on-exec. The late-socket case then motivated the preconnected packet channel and syscall filter. These failures changed implementation; they are not counted as passing negative tests. CLOSE_RANGE_CLOEXEC requires Linux 5.11 or later, bubblewrap 0.12.0 or newer and the required options must exist, and unsupported setup refuses the command.

The final scope audit also rejects hard-linked work files and overlaps with configured state/config roots, and resolves the protected account home through getpwuid_r instead of trusting HOME. This prevents aliases and environment overrides bypassing the work-root checks. Root, set-ID execution and an unavailable account home fail closed. The earlier research considered hard links granted content; this first profile deliberately narrows that proposal.

The syscall ceiling also refuses new sockets in every address family, listeners and ancillary FD transfer. Unnamed AF_UNIX pairs remain for internal IPC. This excludes transport families outside ordinary network namespaces and keeps the inherited TD packet channel usable. All run/setup authority stays in the fixed protected user runtime, never a workload-controlled override. It also refuses the kernel interfaces an unprivileged process can still reach and no offline build needs: the key retention service (the caller's session keyring is otherwise inherited and readable), performance counters, userfaultfd and BPF.

Ubuntu CI caught the allowed-work regression: cc resolves through /etc/alternatives, which was absent. The launch now mounts that system alias directory read-only when present, alongside the linker cache. It does not expose the rest of /etc. The existing compile-and-execute OS probe is the regression gate; denial-only tests would have missed this portability failure.

## Hardening after review (2026-10-21)

A review of the merged profile ran its checks against the installed build and found six routes the first profile left open:
- A nested user namespace could be created.
- The host cgroup path was readable.
- `keyctl` returned the caller's session keyring ID.
- `userfaultfd` returned a descriptor.
- `perf_event_open` and `bpf` both reached the kernel.

bubblewrap now runs with `--disable-userns` and `--unshare-cgroup`, and the syscall filter refuses those four interface families with EPERM. The OS probe asserts each one. On the earlier build every assertion fails, and on this one every assertion passes.

The same review closed one smaller route. The output filter also drops bidirectional controls and the line and paragraph separators, so relayed text cannot display a different command from the one a reader copies.

One review finding did not hold. It reported that an operator client hanging up could kill the supervisor through SIGPIPE, but Rust's standard library already writes Unix stream sockets with MSG_NOSIGNAL. A test that blocks SIGPIPE on its thread and reads it back showed that a plain `write_all` to a hung-up peer raises nothing, so no change was made.

The review completion uses openat/fdopendir from the pinned directory and statx mount IDs, comparing device identity as well. Scan depth and size are bounded. The selected path must be a non-hidden descendant of the OS home. See [the completion decisions and tests](05-review-completion.md). Socket peer credentials are checked against the current pane shell ancestry before own-pane exceptions are installed; hidden document selection and queued input are rechecked at their effects.
