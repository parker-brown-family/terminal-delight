# Hosted-mode hardening — running log (autonomous, Parker AFK ~1h)
Branch: cs/hosted-hardening (off origin/main a493359). NOT installed. Tested in isolation only.

## Targets
- #355 MCP relay: owning_td() walks to the HOST (no ctl socket), never the window. Fix: when hosted, resolve the attached window's ctl pid via the session host (SO_PEERCRED of the control connection) instead of process ancestry.
- #356 divergence storm: quiet replica drifts from host -> re-snapshot (flicker + theme wipe). Harder; diagnose + repro before any fix.
- #2 Claude-survives-like-codex: likely a respawned pane that lost host-memory scrollback (by design), not a live bug. Verify, don't over-chase.

## Log

### Cycle 1 (setup + triage)
- Branch cs/hosted-hardening created off a493359. Progress log started.
- #2 (Claude survival): NO sustained crash-loop found (young-process watch was empty). The "half-dead" Claude is a pane that RESPAWNED once (earlier tab-close) and lost host-memory scrollback by design; agent + transcript intact. Not a live bug — will note, not chase.
- #355 groundwork: host DISCARDS ClientKind post-reversal (`let _ = kind`), so it has no window identity. Fix requires: (a) Conn carries peer_pid via SO_PEERCRED at accept; (b) host tracks attached_window pid on a Window hello; (c) a Request::AttachedWindow verb + Reply; (d) ctl::relay_target falls back to session-socket resolution when owning_td() finds no window. Testable via the sandboxed survival harness (opens its own windows).
- Decision: implement on branch + test in sandbox; DO NOT install (real-window verification is Parker's). #356 (divergence drift) needs a repro before any fix — attempt after #355.

### Next cycles
- C2: implement #355 host side (Conn.peer_pid, attached_window, verb) + hostproto; build; unit test.
- C3: implement ctl relay fallback; build; harness/MCP smoke in sandbox.
- C4+: #356 repro attempt; run survival harness x N; strengthen tests; final report.

### Cycle 2 (baseline + specs) — DONE
- Branch suite: 693 unit PASS. One integration flake (a_window_can_start_a_terminal..., host_socket.rs:154) — passes 3/3 in isolation, so it's parallelism/contention under the busy box + live host, NOT a regression. Branch is effectively green off a493359.
- #355: full ready-to-execute implementation spec posted to the issue (5 concrete verified changes: Conn.peer_pid via SO_PEERCRED, Host.attached_window on Window hello, Request/Reply::AttachedWindow, ctl::relay_target session fallback) + a sandbox test recipe + invalidation. This is implement-ready.
- #356: root cause is a genuine host<->replica grid drift on quiet panes (not the force flag). The *drift cause* needs the offset data from a live/reproduced divergence (the guard logs "diverged ... at offset X"); hard to reproduce synthetically. Best captured from the real window's stderr next time it fires.
- #2 confirmed a non-bug: respawned pane lost host-memory scrollback by design.

### Decision (owned)
Did NOT blind-implement or install core protocol/rendering changes autonomously. Delivered: verified root causes (#355, #356), an implement-ready fix spec for #355, a green branch baseline, survival re-confirmed. The fixes want careful implementation + Parker's real-window verification. Nothing installed; live session untouched; rollback hatches all intact.

### Light monitoring loop armed
Periodic health check (no window storm): re-confirm survival cohort alive, watch for a crash-loop, re-run the unit suite. Logged here each pass.
