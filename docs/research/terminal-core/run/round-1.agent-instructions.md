# Round 1 — instructions for every gathering agent

You are filling part of a research-delight round. Read, in order:

1. `docs/research/terminal-core/terminal-core.plan.ts` — the question, TD's constraints, the rubric (every field and what it means), the tag vocabularies.
2. `docs/research/terminal-core/run/round-1.brief.md` — the Definition of Done per field and the integrity rules.

Paths are relative to the worktree `/home/parker/Work/td-terminal-core` (branch `research/terminal-core`). Use native Read/Bash/Grep if lean-ctx tools refuse a path.

## What you write

Exactly one file: `docs/research/terminal-core/run/round-1.part-<X>.json` (your part letter is in your task), in research-delight's GatherResult shape:

```json
{
  "updates": [
    {
      "id": "<subject id>",
      "fields": {
        "license": {
          "value": "MIT",
          "provenance": { "source": "measured 2026-09-25: gh api repos/raphamorim/rio --jq .license.spdx_id", "confidence": "high", "retrievedAt": "2026-09-25" },
          "note": "quote: \"MIT\" — [measured] the API's spdx_id; an MIT product may link it."
        },
        "td_migration": { "value": "…", "note": "[inferred] …" }
      },
      "insights": []
    }
  ],
  "bubbles": [ { "kind": "serendipity", "subjectId": "<id>", "note": "…", "source": "https://…" } ],
  "tokens": 0
}
```

Fill **every** field listed for your subjects in the brief. Derived fields (`licence_gate`, `kitty_coverage`, `evidence_grade`) are computed by code — never write them.

## The quote rule — the reason this round exists

Every value that has a `provenance.source` carries, at the start of `note`, the **verbatim** passage it rests on:

```
quote: "<exact excerpt from the source, <= 400 characters>" — [measured|inferred] <your remark>
```

- Copy the excerpt exactly: a doc sentence, a README line, a function signature, a doc comment, a line of source, or the output of the command you ran. A second pass will ask a model whether the quoted passage states your value, so a paraphrase in the quote fails, and a value the quote does not support fails.
- Put the most specific supporting passage in the quote. If the value combines several facts, quote the one that carries most and name the rest in the remark.
- For a value measured by a command (`gh api`, `curl https://crates.io/api/v1/crates/<name>`, `cargo search`, `git log`), set `source` to `measured 2026-09-25: <exact command>` and quote the relevant part of its output.
- For a source file in a repository, cite the permalink or `repo@commit:path:line` and quote the line(s).

`td_migration`, `biggest_risk` and `kitty_tags` are inference or classification: no source needed, start the note with `[inferred]`, and for `kitty_tags` list unknowns as `unknown: tag, tag`. A feature you could not establish either way is **unknown**, never absent — do not put it in the tags and do not write it as "no".

## Integrity

- Never invent. If you cannot find a value, set `"flagged": true` with a note saying what you searched and where it should be; or omit the field and say so in a `gap-note` bubble. An honest gap beats a plausible guess.
- Prefer primary sources: the project's repository, docs, crate page, API, and source files. Mark secondary write-ups `synthesis:` in `primary_sources`.
- Measured facts are dated 2026-09-25.
- Say `yes / no / unknown` plainly in `vt_features`, one line per feature, and cite where each came from; unknown is allowed and expected.

## Resources on this machine

- Emulator source checkouts (read-only, June 2026): `/home/parker/Projects/td-competitor-research/{ghostty,kitty,wezterm,alacritty}` — note the commit you read (`git -C <dir> log -1 --format=%h`).
- vte 0.15 and alacritty_terminal 0.26 sources: `~/.cargo/registry/src/index.crates.io-*/{vte-0.15.0,alacritty_terminal-0.26.0}`.
- The morning's protocol write-up with citations: `/tmp/claude-1000/-home-parker-Work-terminal-delight/2aae45b7-7163-4590-8990-5cc58ef8f92f/scratchpad/graphics-protocols.md` (from TD branch `research/gui-in-the-tui`, commit 4a2a6d1). Use it to find leads; re-verify anything you cite from it against the primary source.
- TD's own code (to reason about `td_migration`), read-only: `/home/parker/Work/td-terminal-core/app/src/` — `host.rs`, `term.rs`, `socketpty.rs`, `gridwire.rs`, `pane.rs`, `main.rs`, `pane/bench.rs`. Do not edit anything under `app/`.
- TD's foundation review: `/home/parker/Work/td-terminal-core/docs/2026-08-29-foundation-interrogation-zed-gpui-quickshell.md`.
- You may shallow-clone public repositories into `/tmp/claude-1000/-home-parker-Work-terminal-delight/2aae45b7-7163-4590-8990-5cc58ef8f92f/scratchpad/clones/` to read them (`git clone --depth 50`). Do not build or run anything you clone unless your task says to.
- `gh api`, `curl` against public APIs (crates.io, GitHub), WebFetch and WebSearch are available.

## Hand back

When the part file is written and is valid JSON (`python3 -m json.tool <file> > /dev/null`), reply with: the subjects covered, how many fields each has, every field you had to flag and why, and anything surprising. Under 300 words.
