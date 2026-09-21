#!/usr/bin/env python3
"""What does a hook get handed when CODEX asks a multi-part question?

The companion to reports/_askhook.py, which answered the same question for
Claude Code's AskUserQuestion. Codex's tool is `request_user_input` and its
payload is what decides whether Terminal Delight's agent channel can carry a
round from a model other than Claude.

Reads the same unfiltered store: ~/.lean-ctx/context_radar.jsonl, fed by the
wildcard PostToolUse hook in ~/.codex/hooks.json.

RUN THE CONTROL FIRST. If no non-ctx tool names are present, the store is
filtering and its silence proves nothing -- see reports/_askhook.py.

PERISHABLE, and worse than a ring buffer: two readers minutes apart have seen
disjoint records. A later zero means the window rolled, not that the finding
was wrong. Never grep this file for a tool name; `content` carries whole
payloads including prose that mentions tool names, so a bare grep reports
matches in a file holding no records at all.

    python3 reports/_codexask.py
"""
import datetime
import json
import os

FILES = [
    os.path.expanduser("~/.lean-ctx/context_radar.prev.jsonl"),
    os.path.expanduser("~/.lean-ctx/context_radar.jsonl"),
]
WANTED = ("request_user_input", "AskUserQuestion")
CONTROL = ("Edit", "Bash", "Write", "Read")


def main():
    names = {}
    hits = []
    for path in FILES:
        if not os.path.exists(path):
            continue
        for line in open(path, encoding="utf-8", errors="replace"):
            try:
                obj = json.loads(line)
            except ValueError:
                continue
            name = obj.get("tool_name")
            if not name:
                continue
            names[name] = names.get(name, 0) + 1
            if name in WANTED:
                hits.append((os.path.basename(path), obj))

    print("CONTROL -- non-ctx tools present in the store:")
    for t in CONTROL:
        print("  %-8s %s" % (t, names.get(t, 0)))
    print("  (all zero => the store filters, and nothing below is evidence)")
    print()
    print("ask-tool records found: %d" % len(hits))
    print()

    for base, obj in hits:
        ts = obj.get("ts")
        when = datetime.datetime.fromtimestamp(ts).isoformat(timespec="seconds") if ts else "?"
        print("== %s  %s  %s" % (obj.get("tool_name"), when, base))
        try:
            payload = json.loads(obj.get("content") or "{}")
        except ValueError:
            print("   content is not JSON")
            continue
        print("   payload keys: %s" % sorted(payload.keys()))
        questions = payload.get("questions") or []
        print("   questions in ONE call: %d" % len(questions))
        for n, q in enumerate(questions, 1):
            opts = q.get("options") or []
            print("     Q%d [%s] %s" % (n, q.get("header"), (q.get("question") or "")[:60]))
            print("        id=%s  options=%d  %s"
                  % (q.get("id"), len(opts), [str(o.get("label"))[:16] for o in opts]))
        # The fields Terminal Delight's channel::Asked::parse actually reads.
        need = ("question", "header", "options")
        for n, q in enumerate(questions, 1):
            missing = [k for k in need if k not in q]
            opt_ok = all(isinstance(o, dict) and "label" in o for o in (q.get("options") or []))
            print("     Q%d parses as a TDSP question: %s%s"
                  % (n, not missing and opt_ok,
                     "" if not missing else "  (missing %s)" % ",".join(missing)))
        print("   carries answers: %s   <- true = this is the POST phase"
              % ("answers" in payload or "answer" in payload))
        print()


if __name__ == "__main__":
    main()
