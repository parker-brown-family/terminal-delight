#!/usr/bin/env bash
# Rebuild ownership evidence from committed source history, including renames.
#
# Writing the table scans every branch (`git log --all`), so cleanup can
# recognise a file installed from any checkout. Checking it asks a narrower
# question: does the table cover every version in THIS branch's history and
# the tags? A version committed on some other branch is not yet this branch's
# business; the pull request that brings it here regenerates the table. So a
# push elsewhere can never turn this branch's checks red (issue 1253).
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[[ $# == 0 || ( $# == 1 && $1 == --check ) ]] || {
  echo "Usage: $0 [--check]" >&2; exit 2;
}
python3 - "$REPO" "${1:-}" <<'PY'
import hashlib
from pathlib import Path
import re
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

digests = {}

def history_rows(refs):
    """Every version of every hashed file reachable from `refs`, as table rows."""
    rows = set()
    for current in paths:
        # Follow each discovered spelling too: separate branches can rename the
        # same file differently. Name-only history exposes both sides of renames.
        pending, names = [current], set()
        while pending:
            name = pending.pop()
            if name in names:
                continue
            names.add(name)
            history = git("log", *refs, "--follow", "--format=", "--name-only", "-z", "--", name)
            pending.extend(p.decode() for p in history.split(b"\0") if p and p.decode() not in names)
        blobs = set()
        for name in sorted(names):
            commits = git("log", *refs, "--format=%H", "--", name).decode().splitlines()
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
    return rows

target = repo / "app/src/uninstall/known_hashes.txt"
if sys.argv[2] == "--check":
    if not target.is_file():
        sys.exit(f"Known hashes table missing: {target}; run scripts/known-hashes.sh and commit it.")
    table = target.read_text().splitlines(keepends=True)
    row = re.compile(r"[0-9a-f]{64}  (\S+)\n\Z")
    malformed = [line for line in table if not (m := row.match(line)) or m.group(1) not in paths]
    if malformed:
        sys.exit("Known hashes table has rows that are not '<sha256>  <hashed path>': "
                 + "".join(malformed[:5]).replace("\n", " | "))
    if table != sorted(set(table)):
        sys.exit("Known hashes table is not sorted and unique; run scripts/known-hashes.sh and commit the table.")
    # HEAD's own history plus every tag: what this build, its ancestors and
    # every release could have installed. Missing any of it is a real failure.
    branch = history_rows(["HEAD", "--tags"])
    missing = sorted(branch - set(table))
    if missing:
        sys.exit(f"Known hashes miss {len(missing)} version(s) from this branch's history, e.g. "
                 f"{missing[0].strip()}; run scripts/known-hashes.sh and commit the table.")
    # A new source revision must enter history before regenerating the table.
    uncommitted = [p for p in paths if f"{hashlib.sha256((repo / p).read_bytes()).hexdigest()}  {p}\n" not in table]
    if uncommitted:
        sys.exit("Current shipped files missing from known hashes: " + ", ".join(uncommitted))
    extra = len(set(table) - branch)
    print(f"Known hashes verified: all {len(branch)} versions in this branch's history and tags "
          f"across {len(paths)} files; {extra} more row(s) from other branches kept")
else:
    result = "".join(sorted(history_rows(["--all"]))).encode()
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
    print(f"Wrote {len(result.splitlines())} known hashes to {target}")
PY
