#!/usr/bin/env bash
# Run Ghostty's own libghostty-vt tests (the step its CI calls test-lib-vt) on a copy of the
# source libghostty-vt-sys 0.2.1 pins, with Zig 0.15.2.
#   bash ghostty_lib_tests.sh → prints the tail of the run and its wall time
set -uo pipefail
cache="$HOME/.cache/td-core-research"
q="$cache/quality"
export PATH="$cache/zig-x86_64-linux-0.15.2:$PATH"
export ZIG_GLOBAL_CACHE_DIR="$q/zig-cache-ghostty"
src=$(ls -d "$cache"/targets/ghostbake/release/build/libghostty-vt-sys-*/out/ghostty-src | head -1)
rm -rf "$q/ghostty-src" && cp -r "$src" "$q/ghostty-src" && cd "$q/ghostty-src" || exit 1
start=$(date +%s)
# Debug mode fails to link on this Arch box (Zig 0.15.2's self-hosted linker: "unhandled
# relocation type R_X86_64_PC64" in the bundled C++ objects); Ghostty's CI runs inside Nix.
# OPTIMIZE=ReleaseSafe tries the release path, which is what the cargo build uses.
zig build test-lib-vt ${OPTIMIZE:+-Doptimize=$OPTIMIZE} --summary all 2>&1 | tee "$q/test-lib-vt${OPTIMIZE:+-$OPTIMIZE}.log" | tail -30
echo "exit ${PIPESTATUS[0]}, wall $(( $(date +%s) - start )) s"
