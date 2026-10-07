#!/usr/bin/env bash
# The merged-suite decision, driven with a fake cargo: clean, a flake, a real
# failure, a suite that never reached a result, and a failure that cannot be
# retried by name. Run: bash scripts/lib/merged-suite.test.sh
set -u
here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=merged-suite.sh
. "$here/merged-suite.sh"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin" "$work/app"

# The fake cargo answers by scenario. The whole-suite run prints $SUITE and
# exits $SUITE_EXIT; a retry by name (`-- --exact <name>`) passes when the name
# is listed in $PASSES_ALONE and fails otherwise. Every call is logged.
cat >"$work/bin/cargo" <<'EOF'
#!/usr/bin/env bash
echo "$*" >>"$CALLS"
if [ "${2:-}" = "--locked" ] && [ "${3:-}" = "--no-fail-fast" ]; then
  printf '%b' "$SUITE"
  exit "$SUITE_EXIT"
fi
name="${5:-}"
case " $PASSES_ALONE " in
  *" $name "*) echo "test $name ... ok"; echo "test result: ok. 1 passed; 0 failed"; exit 0 ;;
  *) echo "test $name ... FAILED"; echo "test result: FAILED. 0 passed; 1 failed"; exit 101 ;;
esac
EOF
chmod +x "$work/bin/cargo"
export PATH="$work/bin:$PATH" CALLS="$work/calls"

fails=0
check() { # <name> <want status> <want FLAKED>
  local got=$? want=$2 flaked=$3
  if [ "$got" = "$want" ] && [ "$FLAKED" = "$flaked" ]; then
    echo "ok   $1"
  else
    echo "FAIL $1: status $got (want $want), FLAKED '$FLAKED' (want '$flaked')"
    fails=$((fails + 1))
  fi
}

ok_suite='running 3 tests\ntest a::one ... ok\ntest result: ok. 3 passed; 0 failed\n'
flaky_suite='running 3 tests\ntest a::one ... ok\ntest instance::tests::lock ... FAILED\n\nfailures:\n\n---- instance::tests::lock stdout ----\nthread panicked at src/instance.rs:1111: ws 2 is taken\n\ntest result: FAILED. 2 passed; 1 failed\n'

SUITE=$ok_suite SUITE_EXIT=0 PASSES_ALONE="" merged_suite "$work/app" >"$work/out" 2>&1
check "a clean suite installs" 0 ""

: >"$CALLS"
SUITE=$flaky_suite SUITE_EXIT=101 PASSES_ALONE="instance::tests::lock" merged_suite "$work/app" >"$work/out" 2>&1
check "a test that passes alone twice is a flake, and installs" 0 "instance::tests::lock"
grep -q "ws 2 is taken" "$work/out" || { echo "FAIL the flake's panic is printed"; fails=$((fails + 1)); }
[ "$(grep -c -- '--exact instance::tests::lock' "$CALLS")" = 2 ] || { echo "FAIL the flake is retried exactly twice"; fails=$((fails + 1)); }
grep -q -- '--no-fail-fast' "$CALLS" || { echo "FAIL the whole suite runs with --no-fail-fast"; fails=$((fails + 1)); }

SUITE=$flaky_suite SUITE_EXIT=101 PASSES_ALONE="" merged_suite "$work/app" >"$work/out" 2>&1
check "a test that fails alone too stops the chain" 1 ""

SUITE='error[E0425]: cannot find value `x`\n' SUITE_EXIT=101 PASSES_ALONE="" merged_suite "$work/app" >"$work/out" 2>&1
check "a suite that never reaches a result stops the chain" 1 ""

doc_and_flake='test instance::tests::lock ... FAILED\ntest src/lib.rs - thing (line 3) ... FAILED\ntest result: FAILED. 1 passed; 2 failed\n'
SUITE=$doc_and_flake SUITE_EXIT=101 PASSES_ALONE="instance::tests::lock" merged_suite "$work/app" >"$work/out" 2>&1
check "a failure that cannot be retried by name stops the chain, flake or not" 1 ""

if [ "$fails" = 0 ]; then
  echo "all passed"
else
  echo "$fails failed"
  exit 1
fi
