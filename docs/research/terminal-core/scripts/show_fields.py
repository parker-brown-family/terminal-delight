"""python3 scripts/show_fields.py field [field...] — print each option's raw value for the fields."""
import json
import sys
from pathlib import Path

d = json.loads((Path(__file__).resolve().parent.parent / "jev" / "dossiers.json").read_text())
for x in d["dossiers"]:
    for k in sys.argv[1:]:
        f = x["fields"].get(k) or {}
        print(f"{x['id']:24s} {k}: {str(f.get('value'))[:420]}")
    print()
