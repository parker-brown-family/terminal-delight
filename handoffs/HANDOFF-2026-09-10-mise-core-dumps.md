# Handoff — mise core dumps from the XDG sandbox (2026-09-10)

## Status

Diagnosis complete and proven. Fix **committed, NOT pushed**: `af0295d` on
`left-bar-hardened` in `~/Work/td-client-server`, ahead of `origin` by 1,
worktree clean. No PR. Three sessions converged on this today — `0f0e27f1`
(filed #367 + the file-memories), `8af0071e` (landed `af0295d`), and this one
(the core-dump forensics, #365, the episode).

## What's done

- **Cause established from two cores**, not inferred. Shells spawned under a
  throwaway `/tmp/td-spyd-<pid>` sandbox inherit `XDG_STATE_HOME`, where mise
  keeps its trust database, so `~/Work/.mise.toml` reads as untrusted and the
  `mise activate bash` in every `.bashrc` stops to ask
  `config files in ~/Work are not trusted. Trust them?`. The sandbox then kills
  the window, that question goes to a pty with no reader, and mise panics on the
  `EIO`, panics again reporting it down the same dead stderr, and aborts.
  *Verified:* the composed string `failed printing to stderr: Input/output error
  (os error 5)` is in both cores and **not** in `/usr/bin/mise` (which carries
  only the fragments), so it was formatted at runtime.
- **Reproduced in isolation**, touching none of Parker's mise state, with frames
  `#3`–`#23` byte-identical to the originals. Both conditions are required —
  dead pty **and** untrusted config; the other three combinations exit 0.
- **Fix landed** (`af0295d`, +43 lines): `MISE_CONFIG_DIR`, `MISE_STATE_DIR`,
  `MISE_DATA_DIR` and `MISE_CACHE_DIR` are resolved from the outer environment
  and pinned before the sandbox is applied, in `scripts/td-host-lab.mjs` and
  `scripts/td-survival-test.sh`. XDG stays redirected, so the harnesses still
  cannot touch a real session file.

## How to run/verify

Reproduce the abort. The full dead-pty script is inline in the body of #365
(exits `-6`; **it writes a real core dump**). The short version, from the sibling
session, uses `/dev/full` instead of a dead pty:

```bash
cd /home/parker/Work && MISE_DATA_DIR=/tmp/fresh mise activate bash 1>/dev/null 2>/dev/full
```

Confirm the harness fix holds — after a lab run, this count must not grow:

```bash
coredumpctl list --no-pager | grep -c mise
```

## Not done / next

- **Push/merge `af0295d`.** It is one commit ahead on `left-bar-hardened` and
  nobody has reviewed it. See also #364, the urgent one-install-carrying-both,
  which is the thing actually blocking Parker.
- **#365 — report the abort upstream to `jdx/mise`.** Run its invalidation check
  *first*: this box is on 2026.8.14 and 2026.9.4 is already out, so a current
  mise may exit quietly and there may be nothing to file.
- **#367 — the sandbox side**, still open.

## Watch out

- **The script that actually crashed does not exist.** `td-spyd` and `td-spy2`
  are nowhere in either repo or on disk — the suffixes are PIDs, and the sandbox
  was an inline `env XDG_…=… "$BIN" &` from a session. So `af0295d` hardens the
  two *tracked* harnesses and **cannot be verified against the thing that dumped
  these cores**. The live exposure is the next ad-hoc `env XDG_*=… td &` someone
  types, which no repo-side fix closes.
- **`XDG_DATA_HOME` alone is not the whole trigger.** #367's body attributes the
  pending write to the migrate warning off an empty data dir. Both cores do carry
  `error parsing config file: `, so that path is real — but the runtime-composed
  string in both is the *trust prompt*, which comes off `XDG_STATE_HOME`. Either
  supplies the failing write. `af0295d` pins all four directories, so it is
  unaffected; a fix pinning only `MISE_DATA_DIR` would not have been enough.
- **A third mise core (PID `2074215`, 16:43) is mine**, from the reproduction.
  Removing it needs root; it will otherwise age out.
- **`mise-bin` is stripped with no debuginfod coverage.** Do not spend time on
  gdb here — every frame above libc stays `??`. Read the core's strings and its
  `environ` block instead.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-10-mise-core-dumps-from-the-host-lab-sandbox.md`
- APES tickets: `stop-the-host-harnesses-generating-mise-core-dumps-…-mtw6uhg7` (done),
  `report-upstream-to-jdx-mise-…-mtw6wg19` (backlog, mirrors #365)
- Issues: #365 (upstream report), #367 (sandbox), #364 (urgent, unrelated but blocking)
- lean-ctx: 3 knowledge facts + the session decision
- File-memory: `mise-aborts-on-unwritable-stderr`, `xdg-sandbox-rehomes-every-tool`,
  `lean-ctx-shell-allowlist-traps` (all written by session `0f0e27f1`)
- Harvest: `handoffs/2026-09-10-mise-core-dumps-tieoff.cdx`
