"""Run both specs' trap anchors against live Jev and store the results as data.

    python3 docs/research/terminal-core/scripts/probes.py   → jev/probes.json

Parses `jds probe` output: one line per anchor and question, then the jitter table and
the landed count. The page draws its trap figure from this file, not from memory.
"""
import ast
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
JDS = Path.home() / "Work/jev-decision-spec/bin/jds"
LINE = re.compile(r"^(ok|MISS)\s+(\S+)\s+(\S+)\s+want (\[.*?\])\s+got (\S+)\s+\((.*?)\)\s+\[(\d+)/(\d+)\]\s+→\s+(\S+)")
JIT = re.compile(r"^\s+(\S+\.\S+)\s+([\d.]+)\s*$")
LANDED = re.compile(r"^(\d+)/(\d+) anchors landed")

out = {}
for spec in ("claim-check", "core-fit"):
    path = ROOT / "jev/specs" / spec / f"{spec}.jds.json"
    text = subprocess.run(["python3", str(JDS), "probe", str(path), "--repeats", "3"], capture_output=True, text=True).stdout
    rows, jitter, landed = [], {}, None
    for line in text.splitlines():
        m = LINE.match(line)
        if m:
            rows.append({"ok": m.group(1) == "ok", "anchor": m.group(2), "question": m.group(3),
                         "want": ast.literal_eval(m.group(4)), "got": m.group(5), "detail": m.group(6),
                         "stable": f"{m.group(7)}/{m.group(8)}", "route": m.group(9)})
            continue
        j = JIT.match(line)
        if j:
            jitter[j.group(1)] = float(j.group(2))
        l = LANDED.match(line)
        if l:
            landed = [int(l.group(1)), int(l.group(2))]
    out[spec] = {"anchors": rows, "jitter": jitter, "landed": landed, "raw": text}
    print(spec, landed, f"{len(jitter)} jitter rows")
(ROOT / "jev/probes.json").write_text(json.dumps(out, indent=1))
