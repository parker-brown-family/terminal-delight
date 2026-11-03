#!/usr/bin/env python3
"""Run the two cores and real OS probes serially, with exact source receipts."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import subprocess
from receipt import source, digest

p = argparse.ArgumentParser()
p.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
p.add_argument("--out", type=Path, required=True)
p.add_argument("--target", type=Path, required=True)
a = p.parse_args()
a.repo = a.repo.resolve()
a.out.mkdir(parents=True, exist_ok=False)
env = dict(os.environ, CARGO_TARGET_DIR=str(a.target.resolve()), CARGO_BUILD_JOBS="1")
record = {"source": source(a.repo), "started": datetime.now(timezone.utc).isoformat(), "checks": []}
checks = [
    ("clean-application", ["cargo", "clean", "-p", "terminal-delight"]),
    ("format", ["cargo", "fmt", "--all", "--", "--check"]),
    ("clippy", ["cargo", "clippy", "--locked", "--bins", "--", "-D", "warnings"]),
    ("clippy-fallback", ["cargo", "clippy", "--locked", "--bins", "--features", "core-alacritty", "--", "-D", "warnings"]),
    ("default", ["cargo", "test", "--locked", "--", "--test-threads=2"]),
    ("fallback", ["cargo", "test", "--locked", "--features", "core-alacritty", "--", "--test-threads=2"]),
    ("os", ["cargo", "test", "--locked", "--test", "restricted", "--", "--ignored", "--test-threads=1"]),
    ("release", ["cargo", "build", "--release", "--locked"]),
]
for label, command in checks:
    started = datetime.now(timezone.utc).isoformat()
    with (a.out / f"{label}.log").open("w") as log:
        result = subprocess.run(command, cwd=a.repo / "app", env=env, stdout=log, stderr=subprocess.STDOUT)
    content = (a.out / f"{label}.log").read_text()
    summaries = re.findall(r"test result: .*", content)
    row = {"name": label, "command": command, "started": started, "exit": result.returncode,
           "summaries": summaries, "log_sha256": digest(a.out / f"{label}.log")}
    record["checks"].append(row)
    (a.out / "receipt.json").write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps(row), flush=True)
    if result.returncode:
        print(content[-7000:], flush=True)
        raise SystemExit(result.returncode)
    if source(a.repo)["source_fingerprint"] != record["source"]["source_fingerprint"]:
        raise SystemExit("Source changed during qualification; discard this receipt and rerun")
binary = a.target.resolve() / "release/terminal-delight"
record.update({"finished": datetime.now(timezone.utc).isoformat(), "binary": str(binary), "binary_sha256": digest(binary)})
(a.out / "receipt.json").write_text(json.dumps(record, indent=2) + "\n")
print(a.out / "receipt.json", flush=True)
