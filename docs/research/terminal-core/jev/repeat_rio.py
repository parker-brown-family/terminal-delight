"""Eight identical copies of the rio-vt item: is placements_readable a coin flip?
    python3 repeat_rio.py && jds run specs/core-fit/core-fit.jds.json --items items/rio-repeat.json --out runs/core-fit-rio-repeat
"""
import copy
import json
from pathlib import Path

HERE = Path(__file__).parent
rio = next(c for c in json.loads((HERE / "items" / "cores.json").read_text()) if c["id"] == "rio-vt")
items = []
for i in range(8):
    v = copy.deepcopy(rio)
    v["id"] = f"rio-vt/copy-{i}"
    items.append(v)
(HERE / "items" / "rio-repeat.json").write_text(json.dumps(items, indent=1))
