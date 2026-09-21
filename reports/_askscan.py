#!/usr/bin/env python3
"""Is an AskUserQuestion readable in a transcript while it is still PENDING?

The question under the workbench's live-question path. `live_questions` reads the
terminal grid rather than the transcript because a pending ask is not on disk --
this measures whether that is still true.

Decisive test: find `AskUserQuestion` tool_use blocks with NO matching tool_result
in the same file. Any hit means the harness flushed the question while it was
still being decided, and the whole set (every question, option, description and
preview) is readable without waiting for the person.

Measured 2026-09-21 on the 120 newest transcripts: 43 calls, 19 multi-question,
0 pending. NOTE this is negative evidence -- every ask in those files was
eventually answered, so an orphan only appears if one is abandoned mid-flight.
The conclusive check is to open a picker and grep that session's .jsonl before
answering.

    python3 reports/_askscan.py [--files N]
"""
import argparse
import glob
import json
import os

ROOT = os.path.expanduser("~/.claude/projects")


def scan(paths):
    total = multi = pending = pending_multi = 0
    example = None
    for path in paths:
        uses, results = {}, set()
        try:
            lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
        except OSError:
            continue
        for line in lines:
            try:
                obj = json.loads(line)
            except ValueError:
                continue
            content = (obj.get("message") or {}).get("content")
            if not isinstance(content, list):
                continue
            for block in content:
                if not isinstance(block, dict):
                    continue
                if block.get("type") == "tool_use" and block.get("name") == "AskUserQuestion":
                    uses[block.get("id")] = block.get("input", {})
                if block.get("type") == "tool_result":
                    results.add(block.get("tool_use_id"))
        for tool_id, payload in uses.items():
            total += 1
            questions = payload.get("questions") or []
            if len(questions) > 1:
                multi += 1
            if tool_id not in results:
                pending += 1
                if len(questions) > 1:
                    pending_multi += 1
                    if example is None:
                        example = (os.path.basename(path), tool_id, questions)
    return total, multi, pending, pending_multi, example


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--files", type=int, default=120, help="newest N transcripts")
    args = ap.parse_args()

    paths = sorted(glob.glob(ROOT + "/*/*.jsonl"), key=os.path.getmtime, reverse=True)
    paths = paths[: args.files]

    total, multi, pending, pending_multi, example = scan(paths)
    print("transcripts scanned:      %d" % len(paths))
    print("AskUserQuestion calls:    %d" % total)
    print("  multi-question:         %d" % multi)
    print("  PENDING on disk:        %d   <- any hit invalidates the premise" % pending)
    print("  PENDING and multi:      %d" % pending_multi)
    if example:
        name, tool_id, questions = example
        print("\nexample -- %s  %s" % (name, tool_id))
        for n, q in enumerate(questions, 1):
            print("  Q%d [%s] %s" % (n, q.get("header"), q.get("question", "")[:96]))
            for opt in q.get("options", []):
                preview = opt.get("preview")
                print("      - %-42s %s%s" % (
                    str(opt.get("label"))[:42],
                    str(opt.get("description"))[:56],
                    "  [+preview %d ch]" % len(preview) if preview else "",
                ))


if __name__ == "__main__":
    main()
