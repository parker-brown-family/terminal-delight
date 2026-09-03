#!/usr/bin/env bash
# td-vitals-oracle — diff the Rust vitals against the JS reference implementation.
#
# `app/src/vitals.rs` and `scripts/td-agent-vitals.mjs` compute the same three
# bars from the same transcripts by two independent routes. This asserts they
# still agree.
#
# It exists because these metrics have exactly one failure mode that matters:
# a plausible WRONG number. Nothing crashes, a bar draws, the call it produces is
# confident, and no unit test catches it because both sides are self-consistent.
# Every divergence this has found so far was of that kind, and none of them were
# visible from the spec:
#
#   · seconds vs milliseconds  — sub-second turn gaps dropped out of the latency
#     sample, moving FATIGUE by up to 3 points
#   · the focus boundary read the TURN's timestamp, not the call's — an assistant
#     message with no `usage` block pushes no turn, moving RELEVANCE by 6
#   · the call key joined every prose field instead of reading the first, so a
#     Bash call's changing `description` split repeats of one command
#   · `app/src/` and `app/src` counted as two directories
#   · the jq expression `.conclusion//.state` was swallowed as a path
#
# Requires node and a debug build. Run it after touching either implementation:
#
#   bash scripts/td-vitals-oracle.sh
#
# Real transcripts under ~/.claude/projects are the corpus, so nothing is
# checked in and nothing leaves the machine — it prints only the numbers.
set -u
cd "$(dirname "$0")/.." || exit 1

BIN=app/target/debug/terminal-delight
[ -x "$BIN" ] || { echo "build first: cargo build --manifest-path app/Cargo.toml"; exit 1; }
command -v node >/dev/null || { echo "node is required (the reference is JS)"; exit 1; }

N=${1:-12}
pass=0
fail=0

for f in $(ls -S "$HOME"/.claude/projects/*/*.jsonl 2>/dev/null | head -"$N"); do
  id=$(basename "$f" .jsonl)
  rs=$("$BIN" agent-vitals "$f" 2>/dev/null |
    jq -c '.[0] | {w:(.window*100|round), f:(.fatigue*100|round), r:(.relevance*100|round), c:.call, t:.tokens, l:.limit}')
  js=$(node scripts/td-agent-vitals.mjs "$f" --json 2>/dev/null |
    jq -c '.[0] | {w:(.bars.window|round), f:(.bars.fatigue|round), r:(.bars.relevance|round), c:.call, t:.window.tokens, l:.window.limit}')

  if [ -z "$rs" ] || [ -z "$js" ]; then
    continue # a transcript with no assistant turn yet is not a disagreement
  fi
  if [ "$rs" = "$js" ]; then
    pass=$((pass + 1))
    printf '  ok   %s  %s\n' "${id:0:8}" "$rs"
  else
    fail=$((fail + 1))
    printf '  DIFF %s\n    rust: %s\n    js:   %s\n' "${id:0:8}" "$rs" "$js"
  fi
done

printf '\n%d agree, %d differ\n' "$pass" "$fail"
# Localise a disagreement with the component dumps, which is how every one of
# the above was actually found:
#   app/target/debug/terminal-delight agent-vitals --parts <transcript>
#   app/target/debug/terminal-delight agent-vitals --keys  <transcript>
[ "$fail" -eq 0 ]
