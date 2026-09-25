"""python3 scripts/grep_dossier.py <regex> — every dossier field value or note matching, with its id."""
import json, re, sys
from pathlib import Path
d = json.loads((Path(__file__).resolve().parent.parent / "jev" / "dossiers.json").read_text())
rx = re.compile(sys.argv[1], re.I)
for x in d["dossiers"]:
    for k, f in x["fields"].items():
        for part in ("value", "note"):
            s = str(f.get(part) or "")
            for m in rx.finditer(s):
                a, b = max(0, m.start() - 160), min(len(s), m.end() + 200)
                print(f"{x['id']}/{k}.{part}: …{s[a:b]}…\n")
