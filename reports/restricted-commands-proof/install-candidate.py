#!/usr/bin/env python3
"""Select a reviewed binary for future launches; never signal a live window."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import tempfile


def select(candidate, digest, expected, launcher, library, lockfile, apply=False):
    candidate, launcher, library = Path(candidate), Path(launcher), Path(library)
    if not re.fullmatch(r"td-[0-9a-f]{7,40}-[A-Za-z0-9._-]+", candidate.name):
        raise ValueError("candidate needs its versioned TD source name")
    # Pin the bytes used for installation rather than reopening a checked path.
    content = candidate.read_bytes()
    if hashlib.sha256(content).hexdigest() != digest:
        raise ValueError("candidate digest differs from the reviewed artifact")
    if not launcher.is_symlink() or str(launcher.resolve()) != str(expected):
        raise ValueError("another installation changed the launcher; review its source first")
    destination = library / candidate.name
    result = {"candidate": str(candidate), "sha256": digest, "destination": str(destination),
              "applied": False, "effect": "future launches only; running windows unchanged"}
    if not apply:
        return result
    with Path(lockfile).open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if not launcher.is_symlink() or str(launcher.resolve()) != str(expected):
            raise ValueError("launcher changed while waiting for the deployment lock")
        library.mkdir(parents=True, exist_ok=True)
        if destination.is_symlink():
            raise ValueError("an immutable candidate must be a regular file, not a symlink")
        if destination.exists():
            if hashlib.sha256(destination.read_bytes()).hexdigest() != digest:
                raise ValueError("an existing immutable candidate has different bytes")
        else:
            fd, temporary = tempfile.mkstemp(prefix=".review-candidate-", dir=library)
            try:
                with os.fdopen(fd, "wb") as file:
                    file.write(content)
                    file.flush()
                    os.fchmod(file.fileno(), 0o755)
                    os.fsync(file.fileno())
                # No clobber: an immutable destination is never overwritten.
                os.link(temporary, destination)
            finally:
                Path(temporary).unlink(missing_ok=True)
        fd, temporary = tempfile.mkstemp(prefix=".review-launcher-", dir=launcher.parent)
        os.close(fd)
        Path(temporary).unlink()
        try:
            os.symlink(destination, temporary)
            if str(launcher.resolve()) != str(expected):
                raise ValueError("launcher changed before selection")
            os.replace(temporary, launcher)
        finally:
            Path(temporary).unlink(missing_ok=True)
        result["applied"] = True
        return result


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--candidate", type=Path, required=True)
    p.add_argument("--sha256", required=True)
    p.add_argument("--expected-launcher", required=True)
    p.add_argument("--apply", action="store_true")
    a = p.parse_args()
    if os.getuid() == 0 or os.getuid() != os.geteuid():
        p.error("run as the ordinary account, without sudo")
    home = Path(pwd.getpwuid(os.getuid()).pw_dir)
    runtime = Path(f"/run/user/{os.getuid()}/terminal-delight")
    print(json.dumps(select(a.candidate, a.sha256, a.expected_launcher,
        home / ".local/bin/terminal-delight", home / ".local/lib/terminal-delight",
        runtime / "deploy.lock", a.apply), indent=2))
