# Handoff — client-server slices 3 & 4 (client half) (2026-09-09)

## Status

Landed and pushed. Branch `cs/slice-3`, merged into `client-server-split`
(one PR, **#328**), working tree clean, HEAD level with origin. **Nothing is
installed** — the symlink still holds another agent's build (#327). 707 tests,
`cargo fmt --check` and `clippy -D warnings` clean.

Both halves of the flip gate now have numbers and **both pass**. Slice 5 is a
decision, not an engineering task.

## What's done

| Slice | What landed | Verified by |
|---|---|---|
| 3 | Live-host tier in session resolution; `term::attach_in` over a socket; `Session.shell_pid` → `Option<u32>` with the ripple absorbed at each call site; orphan adoption; divergence guard against the host's `grid-check` | `td-survival-test.sh gui-kill --cycles 20` × 4 runs, **0 losses**; `floor-control` **20/20 lost**; 8 unit tests over a real socket pair |
| 4 (client) | Saves routed to the host (single writer); hosted path claims no flock (#337); new splits cap at 4, tabs still hold 8; host's push feed replaces the window's /proc watcher (#336) | `writer-probe` (adopted tab survives a host checkpoint); lock probe (3 states); `legacy-load` leg (8-pane file opens in full); connection count 13 vs 11 |
| — | `scripts/td-echo-bench.sh` — the flip gate's latency instrument | **95µs** quiet, **75µs** at 8 panes realistic, 0/1000 keystrokes over 1ms |

Defects found *after* the slice was measured, all fixed: a stolen window quit
rather than froze; a genuine exit never reached the window (host half theirs);
two writers made a session clone itself (5 terminals where 3 existed); a
restored backup could start a second agent on one conversation.

## How to run/verify

```bash
cd /home/parker/Work/td-cs-slice3/app && cargo test
```
```bash
cd /home/parker/Work/td-cs-slice3 && ./scripts/td-survival-test.sh gui-kill --cycles 20
```
```bash
cd /home/parker/Work/td-cs-slice3 && ./scripts/td-survival-test.sh floor-control --cycles 5
```
```bash
cd /home/parker/Work/td-cs-slice3 && ./scripts/td-echo-bench.sh
```

Survival legs: `gui-kill` (expect 0), `floor-control` (expect losses ==
cycles — if it reports 0 the instrument is broken), `host-kill`, `pane-exit`,
`legacy-load`. Each opens a window briefly, in a private config and runtime
directory. The bench opens no window.

## Not done / next

- **#339 client half** — send the resume recipe with `spawn-pane` and delete the
  client-side `type_line`. **One commit**: sending while still typing lands the
  line twice. The host half is in; `resume: None` at the call site preserves
  today's behaviour until then.
- ~~#341 — the suite leaves its temp dirs behind~~ **fixed and closed.** One
  guard in `testsync.rs` removing on drop (a failing test panics, so cleanup
  after the assertions is skipped exactly when there is most to skip); the
  accumulated 11,863 dead-pid directories swept. If you add a test helper that
  makes a directory, use that guard — and bind it to a name, because
  `tmp("x").join("f")` drops it at the end of the statement.
- **The eyeball half of the flip gate** — "no visible lag at eight panes" is
  Parker's judgement and nobody has sat in front of it.
- Slice 6 (tie-off issues) untouched.

## Watch out

- **Two agents, split by file.** `host.rs` and `hostproto.rs` belong to the
  session-host agent (`td-client-server-3b`). Message before touching them —
  and measure what their messages claim: two of them turned out to describe
  something worse than they said.
- **Do not install.** #327.
- The saturating-flood number (2.6ms) is real and is deliberately not gated. It
  is the price of two processes for one stream. Do not "fix" it without reading
  the closed #340 first.
- `client-server-split` is checked out in `~/Work/td-client-server`, so it
  cannot be checked out here; pushes go `git push origin HEAD:client-server-split`.
- `/tmp/td-review-target` is a 7.0GB cargo target directory belonging to some
  other session. Not ours, left alone, and worth knowing is there.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-09-client-server-window-attaches.md`
- Gate record: `docs/plans/client-server/00-status.md` (slices 3 and 4 marked done, both gate numbers)
- Session harvest: `reports/cdx/2026-09-09-client-server-slice-3.cdx`
- lean-ctx: session decision recorded (`ctx_session action=decision`)
- Memory: three new entries under `~/.claude/projects/…-terminal-delight/memory/`
- PR: parker-brown-family/terminal-delight#328
