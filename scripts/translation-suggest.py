#!/usr/bin/env python3
"""Suggest translations for on-screen English a change adds. Never fails.

    scripts/translation-suggest.py [BASE [HEAD]]

BASE defaults to origin/main. HEAD defaults to the tree you would get by merging
this branch into BASE (git merge-tree), so a branch that merged an older main is
judged on what it changes now and not on main's old lines. CI passes the pull
request's base commit and its merge commit, which is the same thing.

Terminal Delight draws every word a person reads from the language pack
(app/src/lang.rs and app/src/lang/{coord,chrome,parts}.rs), in nine languages.
A change that adds a skin, a design, a button or a label in English draws that
English in every language until somebody adds it to the pack. Nothing else
notices: the compiler only checks fields that already exist, and the guard test
only checks what is already in the tables.

So this reads the diff and names two kinds of addition:

  - a string literal added inside a drawing call (.child, a tooltip, palette
    help, a label) or as a label-shaped match arm ("ON PACE", "NO READING");
  - a new skin or design file, whose caption and help line belong in the pack;

It is a suggestion, not a gate: it always exits 0. In GitHub Actions it writes
warnings onto the pull request's changed lines and a how-to into the run
summary; anywhere else it prints the same list. docs/TRANSLATING.md is the how.
"""

import os
import re
import subprocess
import sys

UI_CALL = re.compile(
    r"\.child\(|tooltip|Tooltip|palette_help|\.label\(|label\(|\.title\(|\.placeholder\(|SharedString::from\("
)
# A label-shaped match arm: `=> "ON PACE",` or `=> "Light screen; no glow.",`
ARM = re.compile(r'=>\s*"([^"]*)"')
LITERAL = re.compile(r'"((?:[^"\\]|\\.)*)"')
# Lines that print to a log, a socket, a file or an agent: those stay English.
NOT_DRAWN = re.compile(
    r"log::|tracing::|eprintln!|println!|panic!|unreachable!|expect\(|assert|anyhow!|bail!|"
    r"env::var|Command::new|\.arg\(|serde|#\[|json!|write!|writeln!|debug!|info!|warn!|error!"
)
WORDS = re.compile(r"[A-Za-z]{3,}")
TABLES = ("app/src/lang.rs", "app/src/lang/")  # the pack itself


def git(*args):
    return subprocess.run(["git", *args], capture_output=True, text=True).stdout


ID = re.compile(r"[a-z0-9]+(?:[-_][a-z0-9]+)+")


def drawn_words(lit):
    """True when a literal carries words a person reads, not only placeholders,
    symbols, keys or a path."""
    text = re.sub(r"\{[^}]*\}", "", lit)
    if ID.fullmatch(text):
        return False
    if not WORDS.search(text):
        return False
    if re.fullmatch(r"[\w.\-/]+\.(toml|json|rs|md|html|png|svg|sock|log)", text):
        return False
    if text.startswith(("/", "~/", "http", "file:", "--", "td-", "TD_")):
        return False
    return True


def test_spans(path, ref):
    """The line ranges of the file's `#[cfg(test)] mod … {` blocks in `ref`. A
    `#[cfg(test)]` on a single function is not a module and starts nothing."""
    lines = git("show", f"{ref}:{path}").splitlines()
    spans = []
    for i, line in enumerate(lines):
        if line.strip() != "#[cfg(test)]" or i + 1 >= len(lines):
            continue
        nxt = lines[i + 1]
        if not re.match(r"\s*(pub(\([^)]*\))? )?mod \w+ \{", nxt):
            continue
        indent = nxt[: len(nxt) - len(nxt.lstrip())]
        end = next((j for j in range(i + 2, len(lines)) if lines[j] == indent + "}"), len(lines) - 1)
        spans.append((i + 1, end + 1))
    return spans


def main():
    base = sys.argv[1] if len(sys.argv) > 1 else "origin/main"
    if len(sys.argv) > 2:
        head = sys.argv[2]
    else:
        merged = subprocess.run(["git", "merge-tree", "--write-tree", base, "HEAD"], capture_output=True, text=True)
        head = merged.stdout.split()[0] if merged.returncode == 0 and merged.stdout else "HEAD"
    rng_args = [base, head]
    found = []  # (file, line, message)

    # New skins and designs.
    for line in git("diff", "--name-status", *rng_args, "--", "app/skins", "app/themes").splitlines():
        status, _, path = line.partition("\t")
        # A branch that merged an older main can show a file main already has as
        # added against the merge base; only a file the base does not have is new.
        on_base = subprocess.run(["git", "cat-file", "-e", f"{base}:{path}"], capture_output=True).returncode == 0
        if status.startswith("A") and path.endswith(".toml") and not on_base:
            kind = "skin" if path.startswith("app/skins") else "design"
            name = os.path.basename(path)[:-5].replace("-", "_")
            pack = git("show", f"{head}:app/src/lang/chrome.rs") + git("show", f"{head}:app/src/lang.rs")
            caption = f"pal_skin_{name}:" if kind == "skin" else f"thm_{name}:"
            if caption in pack:
                continue
            found.append((path, 1, f"New {kind} '{os.path.basename(path)[:-5]}': its caption and help line "
                          "need fields in app/src/lang/chrome.rs (pal_skin_* / thm_*, pal_help_*) in every language, "
                          "and an arm in the picker's caption match."))

    # Added lines, per file, with their new line numbers.
    diff = git("diff", "-U0", *rng_args, "--", "app/src")
    path, new_line, tests = None, 0, []
    for raw in diff.splitlines():
        if raw.startswith("+++ "):
            path = raw[6:] if raw.startswith("+++ b/") else None
            tests = test_spans(path, head) if path else []
            continue
        m = re.match(r"@@ -\S+ \+(\d+)", raw)
        if m:
            new_line = int(m.group(1))
            continue
        if not path or not raw.startswith("+") or raw.startswith("+++"):
            continue
        line = raw[1:]
        here = new_line
        new_line += 1
        stripped = line.strip()
        if stripped.startswith("//"):
            continue
        if path.startswith(TABLES):
            # A new field must be filled in all nine tables (the compiler) with a
            # real translation (the guard test), so the tables need no reminder.
            continue
        if any(a <= here <= b for a, b in tests):
            continue
        if NOT_DRAWN.search(line):
            continue
        hits = []
        if UI_CALL.search(line):
            drawn = re.sub(r'\.id\("[^"]*"\)', "", line)
            hits = [lit for lit in LITERAL.findall(drawn) if drawn_words(lit)]
        for arm in ARM.findall(line):
            if drawn_words(arm) and not arm.startswith("(") and (arm.isupper() or " " in arm) and arm not in hits:
                hits.append(arm)
        for lit in hits:
            found.append((path, here, f'On-screen English "{lit}": draw it from the language pack '
                          "(a field in lang/chrome.rs for main.rs, lang/parts.rs for other modules) "
                          "so it reads in every language."))

    in_actions = os.environ.get("GITHUB_ACTIONS") == "true"
    for f, n, msg in found:
        if in_actions:
            print(f"::warning file={f},line={n},title=Translation suggested::{msg}")
        else:
            print(f"{f}:{n}: {msg}")
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a") as out:
            if found:
                out.write(f"## Translation suggested ({len(found)})\n\n"
                          "This change adds text a person reads. It is drawn in English in all nine languages "
                          "until it is in the language pack. Not a blocker: add the strings now or in a follow-up. "
                          "How: `docs/TRANSLATING.md`.\n\n")
                for f, n, msg in found:
                    out.write(f"- `{f}:{n}` — {msg}\n")
            else:
                out.write("## Translation suggested\n\nNo new on-screen English found.\n")
    if not found and not in_actions:
        print("no new on-screen English found")
    return 0


if __name__ == "__main__":
    sys.exit(main())
