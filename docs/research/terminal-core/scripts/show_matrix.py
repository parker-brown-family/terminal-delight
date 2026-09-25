"""Print the matrix rows compactly: python3 scripts/show_matrix.py [ids...]"""
import json
import sys
from pathlib import Path

m = json.loads((Path(__file__).resolve().parent.parent / "jev" / "matrix.json").read_text())
ids = sys.argv[1:] or m["ranked"]
dims = list(m["weights"])
print(f"{'':24s}" + " ".join(f"{d[:9]:>9s}" for d in dims) + "   score cover")
for o in ids:
    r = m["rows"][o]
    cells = []
    for d in dims:
        s = r["dims"][d]["score"]
        cells.append(f"{'·':>9s}" if s is None else f"{s:9.2f}")
    sc = r.get("score")
    print(f"{o:24s}" + " ".join(cells) + (f"   {sc:.2f}  {r.get('coverage', 0):.2f}" if sc is not None else "   blocked"))
for o in ids:
    r = m["rows"][o]
    print(o, "|", "; ".join(f"{d}={r['dims'][d]['shown']}" for d in ("trajectory", "bus_factor", "build", "vt_features", "other_protocols", "pictures")))
