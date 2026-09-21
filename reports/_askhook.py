#!/usr/bin/env python3
"""Is AskUserQuestion in Claude Code's hook dispatch path, and what does a hook get?

Settled 2026-09-21 without touching settings.json. Two PostToolUse hooks are
registered with matcher `.*` (`herd hook`, `lean-ctx hook observe`), so any tool
in the dispatch path has been passing through them since they were installed.
The question is only which store kept the tool identity.

  ~/.local/state/herd/state.json   session state only -- no tool names at all
  ~/.lean-ctx/sessions/*           ctx_* only          -- filtered
  ~/.lean-ctx/tool-calls.log       ctx_* only          -- filtered
  ~/.lean-ctx/events.jsonl         ctx_* only          -- filtered
  ~/.lean-ctx/context_radar.jsonl  UNFILTERED          <- the one that answers

Run the control first: if Edit / Bash / Write are absent from the radar too, the
radar filters as well and its silence proves nothing.

WHAT THE RADAR DOES NOT SAY. It has six keys -- ts, tokens, event_type, content,
tool_name, detail -- and none of them is a hook phase. `event_type` is lean-ctx's
own label (a built-in like AskUserQuestion still reads `mcp_call`), so the file
shows the payload was observed COMPLETE; it does not record which hook delivered
it. A payload carrying `answers` alongside `questions` is the PostToolUse
signature, but that is read off the shape, not off a label. The claim that
survives review is "a wildcard PostToolUse hook is the only registered thing that
could have handed lean-ctx this, and it arrived complete".

NEVER grep this file for a tool name. `content` carries whole payloads including
prose, so a bare grep counts other tools' payloads that MENTION the name. The
invariant, not the ratio, is what to remember:

    grep reports matches in a file that contains no records at all.

Three readings during one hour on 2026-09-21 -- 2 records / 237 string hits,
then 0 records / 13 hits, then 0 records / 7 hits. The multiplier is itself a
window and is not worth quoting; the sign is permanent. Parse the JSON.

PERISHABLE, and worse than a plain ring buffer. Two sessions reading an hour
apart saw DISJOINT AskUserQuestion records (2 questions / previews 659,659,659
and 315,199,254 against 1 question / previews 555,371,353), and the whole-store
counts moved in both directions between reads. One of the two reads also found no
rotated file at all. Every number here is a window, not a population -- snapshot
before quoting, and read a later zero as the window having rolled.

    python3 reports/_askhook.py
"""
import json
import os
import datetime

FILES = [
    os.path.expanduser("~/.lean-ctx/context_radar.prev.jsonl"),
    os.path.expanduser("~/.lean-ctx/context_radar.jsonl"),
]
CONTROL = ("Edit", "Bash", "Write", "Read", "Task", "SendMessage")


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
            if name == "AskUserQuestion":
                hits.append((os.path.basename(path), obj))

    print("CONTROL -- non-ctx harness tools present in the radar:")
    for t in CONTROL:
        print("  %-14s %s" % (t, names.get(t, 0)))
    print("  (all zero => the radar filters too, and nothing below is evidence)")
    print()
    print("distinct tool_name values: %d" % len(names))
    print("AskUserQuestion records:   %d" % len(hits))
    print()

    for base, obj in hits:
        ts = obj.get("ts")
        when = datetime.datetime.fromtimestamp(ts).isoformat(timespec="seconds") if ts else "?"
        try:
            payload = json.loads(obj.get("content") or "{}")
        except ValueError:
            print("-- %s %s  content not JSON" % (base, when))
            continue
        questions = payload.get("questions") or []
        print("-- %s  %s  payload keys=%s" % (base, when, sorted(payload.keys())))
        print("   questions: %d" % len(questions))
        for n, q in enumerate(questions, 1):
            opts = q.get("options") or []
            previews = [len(o.get("preview") or "") for o in opts]
            print("     Q%d [%s] %s" % (n, q.get("header"), (q.get("question") or "")[:66]))
            print("        options=%d  preview chars=%s" % (len(opts), previews))
        print("   carries 'answers': %s   <- the PostToolUse signature, inferred"
              % ("answers" in payload))
        print("   hook phase recorded: NO -- the radar has no such field")
        print()


if __name__ == "__main__":
    main()
