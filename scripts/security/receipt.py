"""Source-bound receipts shared by the local security reproducers."""
import hashlib
import json
from pathlib import Path
import subprocess


def source(repo):
    repo = Path(repo).resolve()
    paths = set()
    for directory in ["app/src", "app/tests", "scripts/security"]:
        paths.update(p for p in (repo / directory).rglob("*") if p.is_file() and "__pycache__" not in p.parts)
    paths.update(repo / p for p in ["app/Cargo.toml", "app/Cargo.lock", ".github/workflows/ci.yml"])
    files = {str(p.relative_to(repo)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(paths)}
    return {
        "head_at_recording": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
        "source_files": files,
        "source_fingerprint": hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest(),
    }


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()
