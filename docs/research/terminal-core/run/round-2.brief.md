# Research round 2 — "terminal-core-candidates"

Subjects below are NOT yet complete. Fill **only** their listed gap fields.

## Definition of Done (per field)
- **what_it_is** (required) — what_it_is [source-required] — one or two sentences: what this core/option is and who maintains it
- **license** (required) — license [source-required] — SPDX id, measured with `gh api repos/O/R --jq .license.spdx_id` or the crate's license field; say whether an MIT product may link it
- **language** (required) — language [source-required] — implementation language(s)
- **integration_path** (required) — integration_path [source-or-flag] — how TD would consume it: crates.io crate | git dependency | C ABI through Rust bindings (name the binding crate) | fork + [patch] | not a library (reference only)
- **build_requirements** (required) — build_requirements [source-or-flag] — every toolchain and system library beyond stable cargo (e.g. Zig with version, C/C++ compiler, cmake, network access at build time); 'cargo only' if none
- **latest_release** (required) — latest_release [source-required] — version and date of the newest release TD could depend on (crates.io API, GitHub releases, or tag)
- **first_release** (preferred) — first_release [source-required] — date of the first published release of the embeddable form
- **releases_last_12mo** (preferred) — releases_last_12mo [source-required] — count of releases in the 12 months before 2026-09-25, measured from crates.io versions or GitHub tags; give the command
- **api_stability** (required) — api_stability [source-or-flag] — the project's own words about API stability (quote it), or the semver situation (pre-1.0? breaking changes in recent releases?)
- **maintainers** (required) — maintainers [source-required] — people carrying the work: number of contributors with >= 10% of commits in the last 12 months, measured (gh api repos/O/R/stats/contributors or commits); name the lead maintainer
- **adoption** (preferred) — adoption [source-required] — downloads (crates.io), dependents, and named products that embed it
- **kitty_graphics** (required) — kitty_graphics [source-required] — what of the Kitty graphics protocol it implements, citing source files: transmit media, formats, direct and virtual placements, deletes, query replies, z-layers, animation, quotas. 'none' if none, with the evidence that it drops APC
- **kitty_tags** (required) — kitty_tags — comma-separated tags from exactly this vocabulary for what is IMPLEMENTED (not planned): transmit-direct, transmit-file, transmit-temp, transmit-shm, format-png, format-raw, compression, place-direct, place-virtual, delete-by-id, delete-by-position, query-reply, z-layers, animation — or 'none'. Unknowns are listed in the note as 'unknown: tag, tag'. Drives the derived kitty_coverage.
- **sixel** (required) — sixel [source-or-flag] — Sixel support: yes / no / partial, with the source file or doc
- **iterm2_images** (required) — iterm2_images [source-or-flag] — iTerm2 OSC 1337 inline images: yes / no / partial
- **picture_anchoring** (preferred) — picture_anchoring [source-or-flag] — how a placed picture is tied to the grid so it scrolls, survives scrollback and reflow: tracked pin, absolute row counter, per-cell attribute, placeholder cells, or n/a; and its storage quota
- **vt_conformance** (preferred) — vt_conformance [source-or-flag] — published conformance evidence: vttest / esctest results, a conformance matrix, or the project's stated coverage
- **vt_features** (required) — vt_features [source-or-flag] — for each of kitty-keyboard, osc8-hyperlinks, sync-output-2026, osc52-clipboard, grapheme-clusters, reflow-on-resize, alt-screen, bracketed-paste, mouse-sgr: yes / no / unknown, one line, citing where each was established
- **performance_evidence** (preferred) — performance_evidence [source-or-flag] — published or measured parse throughput and memory per cell; name the benchmark and the machine
- **grid_api** (required) — grid_api [source-required] — how an embedder reads the screen: the types and methods for iterating rows/cells and what a cell carries (char, zero-width/grapheme, fg/bg, underline colour, flags, wide spacer), the cursor, the modes. Quote the doc comment or signature
- **damage_tracking** (preferred) — damage_tracking [source-or-flag] — does it report which rows changed since the last read
- **state_snapshot** (required) — state_snapshot [source-or-flag] — can the full terminal state be serialized, snapshotted, or replayed for a client that attaches later (TD's host/window replica)? Name the API, or 'no'
- **threading_and_loop** (required) — threading_and_loop [source-or-flag] — who owns the read/parse loop (embedder or the library), thread-safety (Send/Sync, locks), and whether it includes a PTY layer
- **reply_channel** (required) — reply_channel [source-or-flag] — how answers to program queries (device attributes, window-size reports, Kitty a=q) reach the embedder so it can write them to the PTY exactly once
- **td_migration** (required) — td_migration — INFERRED: what TD would have to rewrite to adopt this, in terms of TD's seven alacritty-importing files (host.rs, term.rs, socketpty.rs, gridwire.rs, pane.rs, main.rs, pane/bench.rs) — label it inferred
- **biggest_risk** (required) — biggest_risk — INFERRED: the one thing most likely to make this choice a regret, in one sentence
- **primary_sources** (required) — primary_sources [source-required] — space-separated URLs actually read this session; mark secondary write-ups 'synthesis:'

## Integrity rules
- Never invent a value (phone, name, figure). If unfound, omit it or set `flagged: true` with a note like "verify by phone".
- Every value must carry `provenance.source` (a URL) OR be `flagged`. Unsourced, unflagged values are rejected by the harness.
- Only fill the gap fields listed for each subject — do not re-research satisfied fields.
- Emit a `bubbles` entry of kind "serendipity" for anything useful you find that is outside the requested fields.

## Gaps to fill

### B · Swap to a Rust core
- **TD writes its own emulator core on a parser (prior art: Warp, Zellij)** `(td-own-core)` → latest_release, first_release, releases_last_12mo

## Return shape (JSON `GatherResult`)
```json
{
  "updates": [
    {
      "id": "td-own-core",
      "fields": {
        "<field>": {
          "value": "<value>",
          "provenance": {
            "source": "https://…",
            "confidence": "high"
          }
        },
        "<unsourced-field>": {
          "value": "<value>",
          "flagged": true,
          "note": "verify by phone"
        }
      }
    }
  ],
  "bubbles": [
    {
      "kind": "serendipity",
      "subjectId": "<id>",
      "note": "…",
      "why": "…",
      "source": "https://…"
    }
  ],
  "tokens": 0
}
```