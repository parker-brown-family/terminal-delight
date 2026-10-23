# Status: the War Room fence — make the chart a real boundary

**Difficulty: 7/10 — one combined plan, approved by the work itself.** A security
boundary is wrong for a long time before anyone notices, it crosses a process
boundary (Terminal Delight gating what reaches another agent's PTY), and the
copy-hardening piece spans multiple agent vendors. The individual fixes are
contained, so this runs as one plan with a tracer-first build rather than four
full gates.

## Where this came from

The 2026-10-23 security review (`reports/2026-10-23-war-room-security-review.html`,
in the primary checkout) found the War Room is a solid **reading** boundary but not
an isolation one: the control socket (`ctl`) and the MCP **write** verbs reach a
pane without consulting the chart, so any agent with a shell can prompt another
agent, forge the person's approval, or write to a seated pane it has no line to.

Parker concurred on all five decisions and set the bar: *"the fence is not just
useful UX, it is a secure boundary (mostly) … AS LOCKED UP AS WE CAN REASONABLY
MAKE IT."* Done now, with *"lots of very well written tests to make it very apparent
what the surface is."* His three shaping answers:

- **Both gates, layered** (not one): a peer-identity check *and* the scripted escape
  hatch. They are complementary, not redundant, and carry ~zero friction for real
  human use.
- **Hooks-off won't generalise across harnesses** (`disableAllHooks` is Claude-only);
  the durable cross-harness guarantee is an OS sandbox (bubblewrap) around every copy.
  Codex/Grok/openclaw/anything: *"Kinda needs to be [certain]."*
- **Do the work now, with tests.** No issue-filing-and-deferring.

## The finding → fix map

| Review finding | Fix | Slice |
|---|---|---|
| #1 `ctl bench say/type/choose/submit` prompt any Open agent pane, no line | the control-socket fence | 1 — DONE |
| #2 `ctl doc note/concur/save` forge the person's approval | the control-socket fence | 1 — DONE |
| #5 `ctl adopt`/`tabs` ungated (spawn panes, spoof chart identity) | the control-socket fence | 1 — DONE |
| #3 MCP write verbs never check the chart (note/deliverable/appearance/surface) | `Snapshot::write_gate`, called by all four verbs | 2 — DONE |
| #6 the "read-only" copy runs user/plugin/repo hooks before any check | per-CLI flags now + OS (bwrap) jail | 3 — DEFERRED |
| #1/#11 `$TD_TAG` is session-wide, forgeable; rail-card text un-sanitised | per-pane tag; strip C0/bidi/zero-width, cap `why`, recompute colour | 4 — DEFERRED |

## Slice 1 — the control-socket fence (DONE)

`app/src/ctl.rs`, `app/src/main.rs`. The chart gates the MCP read verbs; this gates
the socket the same way. A caller whose process **descends from one of this window's
pane shells** — i.e. an agent running inside a pane — is refused every *person-acting*
verb (`bench *`, `doc *`, `adopt`, `tabs`) unless the window was launched
`TD_MCP_SCRIPTED=1`. An outside operator (the rigs, a person's own shell elsewhere)
and a scripted window still pass.

- The peer pid is the kernel's (SO_PEERCRED), not anything the caller claimed; the
  shell set is the window's own live list, republished by the control ticker each
  pass (swap-safe — it is the window's view of its panes, not a guessed ancestry).
- `is_person_acting` is the classification; the read-only status/appearance verbs
  (`ping`, `whoami`, `*status`, `paint`, `skin`) are deliberately not fenced.
- Tests: `a_person_acting_verb_from_inside_a_pane_is_refused`,
  `the_fence_lets_an_outside_operator_through`,
  `a_scripted_window_lets_an_in_pane_caller_act`,
  `person_acting_covers_every_cross_pane_verb`,
  `caller_inside_a_pane_reads_real_ancestry`.

## Slice 2 — the MCP write-verb chart gate (DONE)

`app/src/mcp.rs`. One shared `Snapshot::write_gate(pid)` the four write verbs
(`set_pane_config`, `leave_note`, `declare_deliverable`, `present_surface`) call
before `apply`. It routes through the same `warroom::may_write` the reading verbs
use: the caller's own pane and an unseated pane under the policy pass; a seated pane
with no line does not. A `Hidden` pane answers **identically to an absent pid**
(`no pane with pid N`), so the gate is no existence oracle; a visible-but-read-only
pane is named. The rule lives in one place so a future verb can't re-open the hole.

- Test: `a_seated_pane_is_safe_from_every_writing_verb_without_a_line` (the writing
  twin of the existing reading test). Planned break: weaken `write_gate` and the
  first block goes red.

## Slice 3 — the cross-harness read-only copy jail (DEFERRED, its own PR)

Parker's Q3 makes this a design decision, not a one-liner. `--settings
disableAllHooks` / `--restricted` are a cheap Claude-only first layer; the guarantee
is an **OS sandbox around every copy process** (bubblewrap 0.12 is on the box),
giving a read-only FS view and no network regardless of harness or hooks. This
subsumes the per-CLI flags and covers Codex/Grok/openclaw/anything. It needs: the
bwrap invocation and the allowlist of readable paths, a fallback when bwrap is
absent, and tests that a hook inside the jail cannot write or reach the network.
Worth its own gate because getting a sandbox subtly wrong is itself a security risk.

## Slice 4 — human-in-the-loop integrity (DEFERRED)

`$TD_TAG` per-pane (so pane A cannot sign for pane B); strip C0 + bidi overrides
(U+202E et al.) + zero-width + cap length on every string drawn on a rail card
(`access.rs`), splice no agent text inside TD's own sentence; recompute a bypass
card's risk colour when the person changes the prompt count. Each is small; grouped
because they all protect the approval surface (OWASP "Lies-in-the-Loop", Trojan
Source).

## Gates

- fmt, clippy `-D warnings`, and the touched test modules green before the PR.
- Every safety rule added here is broken on its own line to confirm its test fails,
  then restored byte-for-byte (house doctrine).
- One PR into `dev/0.5.0` (slices 1–2). Slices 3–4 follow as their own PRs.
