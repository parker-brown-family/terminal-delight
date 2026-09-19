#!/usr/bin/env python3
"""Resolve a CHANGELOG.md merge conflict by keeping BOTH sides.

Every agent appends its entry to the top of `### Fixed` under `[Unreleased]`, and with a
merge landing every few minutes two of them always collide. The conflict is always
semantically empty: both entries are true and neither replaces the other. This is the
resolution the repository already chose by hand in 76b44eb — "both dial entries stand,
in the order they were fixed".

Ours-then-theirs, which for a prepend-at-top changelog puts the branch's newer entry
above main's. Refuses to write anything it cannot fully resolve.
"""
import re
import sys
from pathlib import Path

MARK = re.compile(
    r"^<<<<<<< .*?\n(?P<ours>.*?)^\|\|\|\|\|\|\| .*?\n.*?^=======\n(?P<theirs>.*?)^>>>>>>> .*?\n"
    r"|^<<<<<<< .*?\n(?P<ours2>.*?)^=======\n(?P<theirs2>.*?)^>>>>>>> .*?\n",
    re.S | re.M,
)


def union(text):
    n = [0]

    def keep(m):
        n[0] += 1
        ours = m.group("ours") if m.group("ours") is not None else m.group("ours2")
        theirs = m.group("theirs") if m.group("theirs") is not None else m.group("theirs2")
        a, b = (ours or ""), (theirs or "")
        if a.strip() == b.strip():          # same edit both sides — one copy
            return a
        return a + b

    return MARK.sub(keep, text), n[0]


def main(argv):
    if len(argv) != 2:
        return "usage: _changelog_union.py <path-to-conflicted-CHANGELOG.md>"
    p = Path(argv[1])
    before = p.read_text()
    if "<<<<<<<" not in before:
        print(f"{p}: no conflict markers, nothing to do")
        return 0
    after, n = union(before)
    for bad in ("<<<<<<<", "=======", ">>>>>>>", "|||||||"):
        if bad in after:
            return (f"{p}: {bad} still present after resolving {n} hunk(s) — refusing to write. "
                    "Resolve this one by hand.")
    p.write_text(after)
    print(f"{p}: kept both sides of {n} conflict hunk(s), ours above theirs")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
