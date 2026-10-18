#!/usr/bin/env bash
set -euo pipefail
cd /home/parker/Work/terminal-delight/.worktrees/pane-sleep
test "$(git branch --show-current)" = perf/pane-sleep
git diff --quiet
git diff --cached --quiet
BASE=2b4a6b3963061f28408d1e43971dfa144b9242f8
CAND=$(git rev-parse HEAD)
DEST=/home/parker/.local/lib/terminal-delight
export CARGO_TARGET_DIR=/home/parker/Work/terminal-delight/app/target
trap 'git switch perf/pane-sleep' EXIT
git switch --detach "$BASE"
cargo build --manifest-path app/Cargo.toml --release --locked > /tmp/td-sleep-build-baseline.log 2>&1
BASE_BIN="$DEST/td-$(git rev-parse --short HEAD)-pane-sleep-baseline"
install -m755 "$CARGO_TARGET_DIR/release/terminal-delight" "$BASE_BIN"
git switch perf/pane-sleep
test "$(git rev-parse HEAD)" = "$CAND"
cargo build --manifest-path app/Cargo.toml --release --locked > /tmp/td-sleep-build-candidate.log 2>&1
CAND_BIN="$DEST/td-$(git rev-parse --short HEAD)-pane-sleep"
install -m755 "$CARGO_TARGET_DIR/release/terminal-delight" "$CAND_BIN"
export BASE CAND BASE_BIN CAND_BIN
python3 - <<'PY'
import os,json,hashlib
from pathlib import Path
out=Path("reports/2026-10-17-pane-sleep-evidence")
record={"baseline_source":os.environ["BASE"],"candidate_source":os.environ["CAND"],"same_checkout_path":str(Path.cwd()),"profile":"normal release --locked; no test features","build_exit_codes":[0,0]}
for label in ("baseline","candidate"):
    binary=Path(os.environ["BASE_BIN" if label=="baseline" else "CAND_BIN"])
    record[label+"_binary"]=str(binary)
    record[label+"_sha256"]=hashlib.file_digest(binary.open("rb"),"sha256").hexdigest()
    (out/("build-"+label+".log")).write_bytes(Path("/tmp/td-sleep-build-"+label+".log").read_bytes())
(out/"build-pair.json").write_text(json.dumps(record,indent=2)+"\n")
print(json.dumps(record,indent=2))
PY
