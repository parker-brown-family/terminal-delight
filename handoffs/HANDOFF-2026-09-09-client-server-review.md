# Client-server split review

Review snapshot: branch `client-server-split`, worktree `/home/parker/Work/td-client-server`, initially reviewed at `b46fa8f`. The branch moved while the review was in progress; the remote-tracking branch advanced by three cleanup commits. Those later commits changed scratch-directory cleanup and did not touch the findings below. The worktree was not edited by the reviewer.

## Verdict

The branch has no reproduced session-loss finding in the reviewed paths, but it still differs from the approved protocol contract in two high-severity ways: control requests are accepted before negotiation, and the approved window-level steal policy is not implemented. A host pane cap is also absent. The concurrency test for persistence does not force overlap, so one green run does not prove the ordering property it claims.

## Findings

### High: protocol negotiation is optional

Evidence: `app/src/host.rs:1619` parses and dispatches every request independently. No connection state records that `Hello` was received, and no request is rejected for arriving first. The integration test at `app/tests/host_socket.rs:170` opens a fresh control connection and sends `close-pane` without a hello; it therefore codifies the gap.

Impact: a mismatched or unversioned client can spawn, resize, close, save, watch, or shut down a host. The approved version-break path—shutdown, checkpoint, then TOML recovery—is not implemented by this control loop.

Reproduce: connect to a disposable host socket and send `{"verb":"close-pane","pane":1}` as the first line. The finding is invalidated only if the host rejects the request for missing negotiation and leaves the pane alive.

### High: window-level steal is not implemented

Evidence: `app/src/host.rs:1629` discards `ClientKind`. The host tracks pane sinks, not the current GUI connection. `app/src/host.rs:761` replaces an individual pane stream, while both control connections and watchers remain alive. Stream connections are routed from their greeting at `app/src/host.rs:1550` and are not associated with the control connection that negotiated `ClientKind`.

Impact: two GUI windows can split pane ownership; both controls can remain live; and a same-uid tool can open `stream <pane>` and steal a pane. This contradicts the approved Gate 3 rule that a `Window` hello drops the previous GUI connection while tool clients never attach or steal.

Reproduce: open two window control connections, attach window A to two panes, then attach window B to one. The finding is invalidated only if A's control connection is dropped and the session is transferred atomically at the window level. Current code replaces only the selected pane sink.

### Medium: the approved host sanity cap is absent

Evidence: Gate 3 specifies a generous host cap of 64 panes. `Host::spawn_pane` at `app/src/host.rs:542` has no cap and the reviewed tree contains no host-cap refusal.

Impact: any same-uid script or erroneous client can spawn terminals until process or memory exhaustion. The GUI's four-pane limit does not protect the host protocol.

Reproduce: send 65 `spawn-pane` requests to a disposable host. The finding is invalidated if request 65 returns a truthful refusal. The current implementation has no such refusal path.

### Medium: persistence concurrency test does not force concurrency

Evidence: `two_things_writing_at_once_still_leave_one_good_file` at `app/src/host.rs:3507` starts eight threads, but has no barrier or controlled overlap. Its own comment says the original fault appeared only twice in forty soak runs.

Impact: a single green invocation can serialize fortuitously and would not establish the ordering property the test names. This is a test gap, not proof that the mutex is wrong.

Reproduce: run the test repeatedly against the parent of the writer-lock commit. The finding is invalidated if every run fails against the parent. A deterministic replacement must make at least two writers reach the vulnerable interval together.

### Low: scratch cleanup has no regression assertion

The newest cleanup change moves test roots into a drop guard but adds no assertion that a root disappears after normal return or panic. The affected behavior tests would pass against the parent; they test host behavior, not cleanup.

Reproduce: run the relevant tests against the cleanup commit's parent and inspect the test roots afterward. If none remain, this finding is invalid.

## Latency evidence and limits

The branch records a realistic-load p99 seam cost of 75µs, with 0/1000 attached samples above one millisecond. Saturation is recorded at 2.6ms and is not used as the gate. The benchmark could not be rerun in this review because the lean-ctx shell allowlist rejected `td-echo-bench.sh` before execution; no allowlist bypass was attempted.

The realistic stand-in is eight panes running `seq 1 200; sleep 0.1`, about 2,000 lines per second per pane. That number does not cover a real compiler or agent burst distribution, GUI event dispatch, GPUI/GPU rendering, visible eight-pane lag, or other machine contention.

## Review boundaries

- No source files were edited.
- No install, commit, merge, issue, or GUI action was performed.
- No critical finding or direct session-loss reproduction was found in the reviewed snapshot.
- The branch was moving during review; re-check the two high findings against the eventual tip before acting on them.
