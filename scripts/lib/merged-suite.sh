# shellcheck shell=bash
# The whole suite on the merged tree, run ONCE, and the decision taken from that
# run — for scripts/land.sh. Sourced, never run.
#
#   merged_suite <app dir>     Runs `cargo test --locked` there (CARGO_TARGET_DIR
#                              as the caller set it). Prints the result lines.
#                              Returns 0 to install, 1 not to. Leaves FLAKED set
#                              to the tests that failed in the suite and passed
#                              alone twice, space-separated, empty when none.
#
# Why once. land.sh used to run the suite twice — once to print it, once inside
# the `if` that decided — and two runs can give two answers. Landing #907 on
# 2026-10-06 the first run failed one test, the second passed, and the build
# was installed with the only trace a FAILED line in the middle of the output
# (#910). One run, one answer.
#
# Why a flake is retried rather than refused. The suite carries known tests that
# fail about once in eight full runs under load and pass alone every time
# (#840, #908). Refusing every landing on one of those trains people to rerun
# until it passes, which is the same silence with more steps. So a failing test
# is run again ALONE, twice: still failing, the chain stops; passing both
# times, it is a flake, its panic is printed here, and land.sh says FLAKED ONCE
# on its last line, where nobody can miss it.

FLAKED=""

# The tests a cargo test output says failed, one per line, each once.
failed_tests() {
  grep -oE '^test [^ ]+ \.\.\. FAILED$' | sed -E 's/^test ([^ ]+) \.\.\. FAILED$/\1/' | sort -u
}

# The captured output of one failed test, as cargo prints it under `failures:`.
panic_of() {
  awk -v want="---- $1 stdout ----" '
    $0 == want { on = 1 }
    on && /^$/ { exit }
    on { print; n++; if (n >= 15) exit }
  '
}

# Run one test alone; succeed only if it ran and passed.
retry_alone() {
  (cd "$1" && cargo test --locked -- --exact "$2" 2>&1) | grep -qE "test result: ok\. 1 passed"
}

merged_suite() {
  local app=$1 out code failed t every
  FLAKED=""
  out=$(mktemp)
  # --no-fail-fast: cargo otherwise stops at the first test binary that fails,
  # so a flake in the unit tests would leave every integration test unrun and
  # the retry below would install a build whose other suites nobody ran.
  (cd "$app" && cargo test --locked --no-fail-fast >"$out" 2>&1)
  code=$?
  grep -E "test result:|FAILED" "$out"
  failed=$(failed_tests <"$out")
  if [ "$code" -ne 0 ] && [ -z "$failed" ]; then
    echo "== the suite stopped before it gave a result (exit $code) — NOT installing"
    tail -15 "$out"
    rm -f "$out"
    return 1
  fi
  # Every failure must be one this can retry by name. A doctest's name has
  # spaces in it; a flake retried beside it would otherwise install past it.
  every=$(grep -E '^test .* \.\.\. FAILED$' "$out" | sort -u | wc -l)
  if [ "$every" -ne "$(printf '%s\n' "$failed" | grep -c .)" ]; then
    echo "== a test failed that cannot be retried by name — NOT installing"
    grep -E '^test .* \.\.\. FAILED$' "$out"
    rm -f "$out"
    return 1
  fi
  for t in $failed; do
    echo "== $t failed in the whole suite; what it said:"
    panic_of "$t" <"$out"
    if retry_alone "$app" "$t" && retry_alone "$app" "$t"; then
      echo "== $t passed alone twice: a flake. Installing, and saying so at the end."
      FLAKED="${FLAKED:+$FLAKED }$t"
    else
      echo "== $t fails alone too — NOT installing"
      rm -f "$out"
      return 1
    fi
  done
  rm -f "$out"
  return 0
}
