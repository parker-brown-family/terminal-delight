#!/usr/bin/env bash
# Rebuild ownership evidence from committed source history, including renames.
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[[ $# == 0 || ( $# == 1 && $1 == --check ) ]] || {
  echo "Usage: $0 [--check]" >&2; exit 2;
}
python3 - "$REPO" "${1:-}" <<'PY'
import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile

repo = Path(sys.argv[1])
paths = (
    "scripts/td-agent-hooks", "scripts/td-agent-ledger", "scripts/td-keybindings",
    "scripts/td-send", "plugins/leanctx-mcp/leanctx-mcp", "plugins/td-send/td-send-mcp",
    "docsite/pages/keys.html", "packaging/terminal-delight.desktop",
    "packaging/terminal-delight.svg",
)

def git(*args):
    return subprocess.check_output(["git", *args], cwd=repo)

if git("rev-parse", "--is-shallow-repository").strip() == b"true":
    sys.exit("Known hashes require complete git history; fetch with --unshallow first.")

rows = set()
digests = {}
for current in paths:
    # Follow each discovered spelling too: separate branches can rename the
    # same file differently. Name-only history exposes both sides of renames.
    pending, names = [current], set()
    while pending:
        name = pending.pop()
        if name in names:
            continue
        names.add(name)
        history = git("log", "--all", "--follow", "--format=", "--name-only", "-z", "--", name)
        pending.extend(p.decode() for p in history.split(b"\0") if p and p.decode() not in names)
    blobs = set()
    for name in sorted(names):
        commits = git("log", "--all", "--format=%H", "--", name).decode().splitlines()
        for commit in commits:
            item = git("ls-tree", commit, "--", name).decode().strip()
            if item:
                meta = item.split("\t", 1)[0].split()
                if meta[1] == "blob":
                    blobs.add(meta[2])
    for blob in sorted(blobs):
        if blob not in digests:
            digests[blob] = hashlib.sha256(git("cat-file", "blob", blob)).hexdigest()
        rows.add(f"{digests[blob]}  {current}\n")

result = "".join(sorted(rows)).encode()
target = repo / "app/src/uninstall/known_hashes.txt"
if sys.argv[2] == "--check":
    if not target.is_file() or target.read_bytes() != result:
        sys.exit("Known hashes differ from history; run scripts/known-hashes.sh and commit the table.")
    # A new source revision must enter history before regenerating the table.
    missing = [p for p in paths if f"{hashlib.sha256((repo / p).read_bytes()).hexdigest()}  {p}\n" not in rows]
    if missing:
        sys.exit("Current shipped files missing from known hashes: " + ", ".join(missing))
    print(f"Known hashes verified: {len(rows)} versions across {len(paths)} files")
else:
    target.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=target.parent, delete=False) as f:
        staged = Path(f.name)
        try:
            f.write(result)
            f.flush()
            staged.chmod(0o644)
            staged.replace(target)
        finally:
            staged.unlink(missing_ok=True)
    print(f"Wrote {len(rows)} known hashes to {target}")
PY
