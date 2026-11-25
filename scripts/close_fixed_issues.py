#!/usr/bin/env python3
"""Close the issues a pull request fixed when it merges into a branch that is
not the default one.

GitHub closes an issue named by "Fixes #N" only when the pull request merges
into the default branch. Work here lands on integration branches (dev/0.5.5),
so every fixed issue stayed open until someone closed it by hand; on
2026-11-18 a triage found ten issues fixed and never closed, one of them for
two weeks. The workflow close-fixed-issues.yml runs this on each pull request
merged into a dev/ branch.

Reads PR_NUMBER, PR_BODY, PR_BASE and GH_REPO from the environment and calls
the gh CLI. --dry-run prints what it would close.
"""
import os
import re
import subprocess
import sys

# GitHub's own closing keywords, a number in this repository only
# (owner/repo#N references to other repositories are left alone).
KEYWORDS = re.compile(
    r"(?<![\w/])(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s*:?\s+#(\d+)\b",
    re.IGNORECASE,
)


def fixed_numbers(body):
    """The issue numbers a pull request body says it fixes, in order, once each."""
    seen = []
    for match in KEYWORDS.finditer(body or ""):
        n = int(match.group(1))
        if n not in seen:
            seen.append(n)
    return seen


def gh(*args):
    return subprocess.run(["gh", *args], capture_output=True, text=True)


def main():
    dry = "--dry-run" in sys.argv
    pr, body, base, repo = (os.environ.get(k, "") for k in ("PR_NUMBER", "PR_BODY", "PR_BASE", "GH_REPO"))
    for n in fixed_numbers(body):
        state = gh("issue", "view", str(n), "--repo", repo, "--json", "state", "--jq", ".state")
        # A pull request number, a missing issue or one already closed is left alone.
        if state.returncode != 0 or state.stdout.strip() != "OPEN":
            print(f"#{n}: not an open issue ({state.stdout.strip() or state.stderr.strip()}), left alone")
            continue
        comment = f"Fixed by #{pr}, merged into {base}. Closed by the close-fixed-issues workflow, because GitHub closes \"Fixes #N\" issues only on merges into the default branch."
        if dry:
            print(f"#{n}: would close ({comment})")
            continue
        done = gh("issue", "close", str(n), "--repo", repo, "--reason", "completed", "--comment", comment)
        print(f"#{n}: {'closed' if done.returncode == 0 else 'NOT closed: ' + done.stderr.strip()}")


if __name__ == "__main__":
    main()
