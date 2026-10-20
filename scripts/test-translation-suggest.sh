#!/usr/bin/env bash
# Prove scripts/translation-suggest.py against real history: what main added in
# English after the translation pass must be named, and two pull requests that
# translated their own strings must be left alone. Needs full history.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 2
# Read the script's local output. Under Actions it would print annotations onto
# this pull request for old history, and its all-clear goes to the job summary.
unset GITHUB_ACTIONS GITHUB_STEP_SUMMARY
S=scripts/translation-suggest.py
fail=0

# Main between the translation branch and 0dcdb741 added the usage pace card,
# the NO READING verdict and the chalk skin, all in English.
got=$(python3 "$S" cadc9fd468f78caa83ceee107727a48bf6b34d73 0dcdb741c75e170b0c2ebd2a361f74fcf6867246)
for want in "TOKEN USAGE PACE" "today, the week, by model" "ON PACE" "NO READING" \
  "Paper and chalk strokes in the current theme colours." "New skin 'chalk'"; do
  grep -qF "$want" <<<"$got" || { echo "MISSED: $want"; fail=1; }
done
for noise in "syntax-off" "palette-follow-outer"; do
  grep -qF "\"$noise\"" <<<"$got" && { echo "FLAGGED AN ELEMENT ID: $noise"; fail=1; }
done

# The bug report (#1042) and the OPS skin (#1040) put every word in the pack.
for merge in d9a420ab 902e4934; do
  got=$(python3 "$S" "$merge^1" "$merge")
  grep -q "no new on-screen English" <<<"$got" || { echo "FALSE ALARM on $merge:"; echo "$got"; fail=1; }
done

[ "$fail" = 0 ] && echo "translation-suggest: all fixtures hold"
exit "$fail"
