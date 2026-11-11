# Isolated session-host laboratory

Both tested binaries retained their synthetic PTYs and passed the initial nine experiments.
The wave binary also passed the expanded eleven-experiment suite.
No Terminal Delight defect was found in these bounded runs. The laboratory
opens no window and discovers or controls no live session.

## Reproduce

```bash
python3 reports/adversarial/host_lab.py --binary /absolute/path/to/terminal-delight --out /tmp/td-host-results.json
```

Choose the binary explicitly. Each experiment starts its own host with private
runtime, config, state, data and cache directories. Its PTYs run `/bin/cat`,
never an interactive shell startup or an agent. Cleanup addresses only the
processes the experiment created. Core dumps are disabled.

## Evidence

- [host-results.json](host-results.json): wave binary, SHA-256 `aaa87fb5a925145b7f5c16a300c9ee8582ef7f2df479933405a4ac1f9f40df1f`.
- [preview-results.json](preview-results.json): preview binary, SHA-256 `8654331d95095b313cf3a1cc2214a46e8620ff356cf13d9e2e91057c42b608d4`.
- [host-expanded-results.json](host-expanded-results.json): adds concurrent/interrupted saves and resize/Unicode probes on the wave binary.
- [host-results-initial.json](host-results-initial.json): first measurements and an erroneous expectation that a duplicate host must return nonzero. The actual protection worked: the original host and socket survived. The corrected test accepts idempotent success and checks ownership.

The checkout used to read the protocol is `ef6dcde5`. It is not established
as the exact source of either installed binary, so `source_binary_match` stays
null. The binary hashes identify what was actually tested.

| Active PTYs | Wave p95 echo, ms | Preview p95 echo, ms | Wave host RSS, MiB | Preview host RSS, MiB |
|---|---|---|---|---|
| 2 | 0.107 | 0.105 | 25.49 | 25.36 |
| 5 | 0.167 | 0.185 | 26.72 | 26.27 |
| 20 | 0.788 | 0.687 | 41.94 | 33.92 |
| 50 | 2.756 | 2.367 | 47.18 | 47.05 |

Each scale receives 12 rounds, one line per PTY per round. A round completes
only after both the terminal's echo and cat's copy arrive. This includes Python
client scheduling and line discipline. RSS covers the host, excluding cat
children and the client. Single runs under ordinary machine load do not
establish a performance difference between the binaries.

## Invariants Tested

1. At 2, 5, 20 and 50 panes, every echo arrives, a disconnect preserves every
   child identity, and reattachment contains the last round and accepts input.
2. Fragmented malformed JSON, unknown verbs and protocol mismatch receive errors;
   a following valid request works. An interrupted request creates no pane.
3. Thirty-two concurrent spawns produce unique pane identities. Killing 16 owned
   children and concurrently closing all 32 leaves an empty census.
4. Thirty-two stalled partial requests do not stop another client receiving a
   reply. After disconnect, host threads return to the starting count.
5. A duplicate host exits and leaves the original host serving the session.
6. Zero columns, rows or cell dimensions do not crash the tested host.
7. Forty concurrent saves produce parseable, internally consistent TOML while
   a concurrent observer reads it. A partially sent save leaves the saved file unchanged.
8. Thirty-two resize/fragmented-UTF-8 rounds make progress and preserve the final
   wide-character marker in a newly attached snapshot.

## What Remains Unmeasured

This does not measure frame time, GPU memory, physical input-to-photon latency,
scrollback rendering, CRT cost, rapid UI splits, a window's reconstruction of a
snapshot, agent transcript parsing or persisted state after host SIGKILL.
Those measurements remain null, not zero. No conclusion about their safety or
performance follows from these results.

The suite reuses the existing host protocol from `app/src/hostproto.rs`, the
snapshot fence used by `app/tests/host_socket.rs`, and the XDG separation of
`scripts/td-host-lab.mjs`. It adds a repeatable multi-PTY workload and records
the chosen executable, rather than introducing another terminal runtime.
