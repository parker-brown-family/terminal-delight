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

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path
import json

UI_CALL = re.compile(
    r"\.child\(|tooltip|Tooltip|palette_help|\.label\(|label\(|\.title\(|\.placeholder\(|\.control\(|\.control_text\(|\bbezel_btn(?:_s)?\(|\bstatus:\s*\""
)
# A label-shaped match arm: `=> "ON PACE",` or `=> "Light screen; no glow.",`
ARM = re.compile(r'=>\s*"([^"]*)"')
LITERAL = re.compile(r'"((?:[^"\\]|\\.)*)"')
# Lines that print to a log, a socket, a file or an agent: those stay English.
NOT_DRAWN = re.compile(
    r"log::|tracing::|eprintln!|println!|panic!|unreachable!|expect\(|assert|anyhow!|bail!|"
    r"env::var|Command::new|\.arg\(|serde|#\[|json!|write!|writeln!|debug!|info!|warn!|error!"
)
WORDS = re.compile(r"[A-Za-z]{2,}")
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
    """The line ranges of test-only items at the selected revision."""
    source = git("show", f"{ref}:{path}")
    return test_spans_source(source)


def code_mask(source):
    """Keep source offsets while hiding comments, strings and character literals.

    Braces inside a raw test fixture must not end its containing function.
    """
    token = re.compile(r'//[^\n]*|/\*[\s\S]*?\*/|r(\#*)"[\s\S]*?"\1|"(?:[^"\\]|\\[\s\S])*"|\'(?:[^\'\\\n]|\\.)\'')
    return token.sub(lambda m: re.sub(r'[^\n]', ' ', m[0]), source)


def test_spans_source(source):
    """Test modules AND standalone test functions, including gpui tests."""
    mask = code_mask(source)
    spans = []
    for annotation in re.finditer(r'#\[(?:cfg\(test\)|(?:gpui::)?test)\]', mask):
        opening = mask.find('{', annotation.end())
        semicolon = mask.find(';', annotation.end())
        if opening < 0 or (0 <= semicolon < opening):
            continue
        depth = 1
        end = opening + 1
        while end < len(mask) and depth:
            depth += (mask[end] == '{') - (mask[end] == '}')
            end += 1
        spans.append((source.count('\n', 0, annotation.start()) + 1, source.count('\n', 0, end) + 1))
    return spans


def drawn_literals(line):
    # An id is not a caption, even when format! builds it over several calls.
    drawn = re.sub(r'\.id\((?:[^()"]|"(?:[^"\\]|\\.)*"|\([^()]*\))*\)', '', line)
    # Placeholder names passed to fill are data, not UI words.
    drawn = re.sub(r'\(\s*"[a-z_]+"\s*,\s*&', '(&', drawn)
    hits = []
    if UI_CALL.search(drawn):
        if '.control(' in drawn:
            controls = re.findall(r'\.control\(\s*"(?:[^"\\]|\\.)*"\s*,\s*"((?:[^"\\]|\\.)*)"', drawn)
            hits.extend(lit for lit in controls if drawn_words(lit))
            drawn = re.sub(r'\.control\([^\n]*?\)', '', drawn)
        hits.extend(lit for lit in LITERAL.findall(drawn) if drawn_words(lit))
    for arm in ARM.findall(drawn):
        if drawn_words(arm) and not arm.startswith('(') and (arm.isupper() or ' ' in arm) and arm not in hits:
            hits.append(arm)
    return list(dict.fromkeys(hits))


def continued_literals(source):
    """Locate text-control words and statuses split across lines by rustfmt."""
    quoted = r'"((?:[^"\\]|\\.)*)"'
    hits = {}
    for pattern, group in [(r'\.control\(\s*' + quoted + r'\s*,\s*' + quoted, 2),
                           (r'\bstatus\s*:\s*' + quoted, 1),
                           (r'\.control_text\(\s*' + quoted, 1),
                           (r'\bbezel_btn(?:_s)?\(\s*[^,]+,\s*' + quoted, 1)]:
        for match in re.finditer(pattern, source):
            literal = match[group]
            if drawn_words(literal):
                line = source.count('\n', 0, match.start(group)) + 1
                hits.setdefault(line, []).append(literal)
    return hits


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('base', nargs='?', default='origin/main')
    parser.add_argument('head', nargs='?')
    parser.add_argument('--all', action='store_true', help='audit every tracked Rust source in the working tree')
    parser.add_argument('--json', action='store_true', help='emit candidate file/line/literal records')
    args = parser.parse_args()
    if args.all:
        found = []
        for path in git('ls-files', 'app/src/*.rs', 'app/src/**/*.rs').splitlines():
            if path.startswith(TABLES):
                continue
            source = Path(path).read_text()
            tests = test_spans_source(source)
            continued = continued_literals(source)
            for n, line in enumerate(source.splitlines(), 1):
                if line.lstrip().startswith(('//', '*')) or NOT_DRAWN.search(line) or any(a <= n <= b for a,b in tests):
                    continue
                for lit in dict.fromkeys(drawn_literals(line) + continued.get(n, [])):
                    found.append(dict(file=path, line=n, literal=lit))
        if args.json:
            print(json.dumps(found, ensure_ascii=False, indent=2))
        else:
            for hit in found:
                print(f"{hit['file']}:{hit['line']}: {hit['literal']}")
        return 0
    base = args.base
    if args.head:
        head = args.head
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
    path, new_line, tests, continued = None, 0, [], {}
    for raw in diff.splitlines():
        if raw.startswith("+++ "):
            path = raw[6:] if raw.startswith("+++ b/") else None
            tests = test_spans(path, head) if path else []
            continued = continued_literals(git('show', f'{head}:{path}')) if path else {}
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
        for lit in dict.fromkeys(drawn_literals(line) + continued.get(here, [])):
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
