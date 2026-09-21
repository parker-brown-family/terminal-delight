# Handoff — lean-ctx savings card + ledger guard (2026-09-19)

## Status

**Landed and merged.** terminal-delight#575 merged at `06:17:49Z` into
`origin/main`; `db3cada` is an ancestor of `569f452`, and the currently
installed build `td-569f452-allin` **already contains it**. Nothing to rebuild
— a restarted window shows it (a new tile is not a new process).

Two follow-ups open, both `follow-up` labelled and dual-written to APES.
Machine-global tooling (the guard) is installed and running outside any repo.

## What's done

**1. The `</>` card no longer opens blank** — terminal-delight#575.
The savings tab chip only assigned `savings_tab` and notified; it never ran the
plugin. Every route into the card that is not the green `</> savings` button
(the `Σ usage` button, ctrl+shift+A, a left-bar allowance rail) lands on the
usage face, so `savings_view` and `savings_status` both stayed `None` and the
body fell off the end of an `if let … else if let …` with no `else`.
*Verified:* `cargo fmt --check`, `cargo clippy --locked -- -D warnings`,
`cargo test --locked` (1,369 tests) all clean; the new gate
`every_door_into_the_savings_face_reads_the_rollup` had both load-bearing legs
broken on purpose and watched to fail.

**2. The number was never low — the ledger was four days old.**
`stats.json.corrupt` is **0 bytes**, mtime `2026-09-15 12:33:44`; the live
ledger's `first_use` is `12:37:31`, four minutes later. 35.1M tokens over 3.44
days is 10.2M/day, ~$51/day, ~$1,500/month. *Verified:* four daily buckets in
`stats.json`; `cost_attribution.json`'s earliest agent matches `first_use` to
the second, so the whole state family reset together.

**3. History reconstructed, not recovered.** 31 Aug – 11 Sep ≈ **82–85M tokens,
~$420**, by two methods agreeing within 3.7%. *Verified:* the method checks out
against the surviving window — 10,897 transcript `ctx_*` calls vs 10,194 ledger
commands, fidelity 0.935, applied to both estimates.

**4. `leanctx-stats-guard` installed and running.** Hourly systemd user timer.
Snapshots a validated gzipped ledger; exits 1 on an unreadable one (snapshots
untouched) and 2 on a **discontinuity** (`first_use` moved or counter
regressed — stored, nothing pruned), so the unit goes red. *Verified:* all five
legs broken deliberately, then re-run through `systemd-run` to confirm the unit
actually fails. Three snapshots held as of writing; the timer has fired on its
own.

**5. `cdx-audit`'s `duplicate-action` is a false positive here** — five flagged
"duplicates" had five distinct bodies (648–2,844 chars). Acked in the baseline
with a note. Evidence landed on **context-delight#17**, which already had the
right mechanism: the dedup key is a 120-char truncation (5 commands → 2 distinct
keys at 120, 5 at 200), not first-line matching. My own #18 asserted first-line
matching, was wrong, and is closed as a duplicate.

## How to run/verify

```bash
leanctx-stats-guard status
```
```bash
leanctx-stats-guard list
```
```bash
systemctl --user list-timers leanctx-stats-guard.timer
```

Check a savings figure's window before quoting it — this is the habit that was
missing:

```bash
python3 -c "import json,os;d=json.load(open(os.path.expanduser('~/.lean-ctx/stats.json')));print(d['first_use'],len(d['daily']),'days')"
```

Prove the guard still has a failing leg (never against the live ledger):

```bash
systemd-run --user --wait --collect -E LEANCTX_DIR=/tmp/fake -E XDG_STATE_HOME=/tmp/fakestate leanctx-stats-guard snapshot
```

## Not done / next

- **terminal-delight#577 — the card states no window.** Needs `first_use`
  plumbed through `leanctx-mcp`'s payload into `SavingsView` and rendered
  beside the headline, with unknown saying unknown. ~20 lines across two files.
  APES: `show-the-ledger-s-window-on-the-savings-card-see-577-mu81it9p`.
- **context-delight#17 — `duplicate-action` 120-char truncation.** Pre-existing;
  this session added the measured threshold and the heredoc case. #18 (mine) was
  closed as a duplicate with a wrong mechanism.
- **lean-ctx's 15-live-worker cap** turns away every agent past the fifteenth
  on one project root; they then use uncompressed tools and save nothing. All 15
  were genuinely alive (no ghosts). No config key or env var found — looks
  upstream, not filed.
- **The 15 Sep corruption trigger is unidentified.** `auto_update` is `false`,
  `diagnostics.json` is empty. The guard makes the next one survivable; it does
  not explain this one.
- `lean-ctx doctor` reports 41/45: the missing shell alias in `~/.bashrc` and
  the inactive daemon unit are real and unfixed; the config-parity warning is
  benign (`LEAN_CTX_DATA_DIR` is pinned, but to the standard path).

## Watch out

- **This whole session ran on native tools.** lean-ctx refused every `ctx_*`
  call from first to last — `agent capacity reached … 15/15 live workers`. That
  is a full queue, not a dead server. Expect it in this repo.
- **Never write the reconstruction into `stats.json`.** It is deliberately
  beside the ledger, typed `"kind": "estimate"`. An estimate inside a
  measurement store is indistinguishable from measurement forever.
- **12–14 Sep is `null`, not `0`** — no ledger and effectively no transcripts
  (session persistence was not forced on until the 15th). Do not let a later
  pass fill it with a zero.
- The guard's snapshot filenames must keep microsecond resolution. At second
  resolution two runs in one second silently overwrite — the tool losing a
  ledger, which is exactly what it exists to prevent. That was a real bug,
  caught only by the deliberate leg-breaking.
- `~/Work/td-savings-chip` was removed after the merge; the branch is deleted
  locally. The commit lives in `main`.

## Where it's recorded

- **APES episode:**
  `apes/projects/terminal-delight/episodes/2026-09-19-the-savings-card-and-the-four-day-odometer.md`
- **APES tasks:** 2 closed, 2 open follow-ups (ids above), 1 in `context-delight`
- **lean-ctx:** *nothing* — `ctx_knowledge` / `ctx_session` were unreachable all
  session (worker cap). This handoff and the episode carry what would have gone
  there.
- **file-memory:** `lean-ctx-refuses-everything-at-worker-capacity.md`,
  `the-leanctx-ledger-resets-silently.md`
- **Harvest:** `handoffs/2026-09-19-leanctx-savings-and-ledger-guard.cdx`
- **Report:** `~/Work/reports/2026-09-19-leanctx-history-reconstruction.md`
- **PR / issues:** terminal-delight#575 (merged), terminal-delight#577 (open),
  context-delight#17 (open, pre-existing — evidence added), context-delight#18
  (mine, closed as a duplicate with a wrong mechanism)
