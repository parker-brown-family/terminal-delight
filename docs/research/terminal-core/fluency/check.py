"""Check one agent's output.json against fluency/ground-truth.json, whatever shape its JSON took.

    python3 check.py <workspace>   → prints the four checks and what was found

The agents chose their own JSON shapes, so this walks the whole document: a placement is any
object with a width-like and a height-like number, the cursor is any object under a key
containing 'cursor', replies are any strings under a key containing 'repl', and history is
any integer under a key containing 'row', 'held', 'total' or 'history' at the top level of
step four's result. What it finds is printed beside the verdict, so a miss can be read.
"""
import json
import sys
from pathlib import Path

ws = Path(sys.argv[1])
try:
    doc = json.loads((ws / "output.json").read_text())
except Exception as e:  # a missing or malformed output is a finding, not a crash
    print(json.dumps({"workspace": ws.name, "output_json": f"unreadable: {e}"}))
    sys.exit(0)


def walk(x, path=()):
    yield path, x
    if isinstance(x, dict):
        for k, v in x.items():
            yield from walk(v, path + (str(k).lower(),))
    elif isinstance(x, list):
        for i, v in enumerate(x):
            yield from walk(v, path + (str(i),))


WIDTH = ("width", "cols", "columns", "w")
HEIGHT = ("height", "rows", "h")
placements, cursors, replies, history = [], [], [], []
for path, v in walk(doc):
    if isinstance(v, dict):
        keys = {str(k).lower(): val for k, val in v.items()}
        wk = next((k for k in keys if any(k == w or k.endswith("_" + w) or k.startswith(w) for w in WIDTH) and isinstance(keys[k], (int, float))), None)
        hk = next((k for k in keys if any(k == h or k.endswith("_" + h) or k.startswith(h) for h in HEIGHT) and isinstance(keys[k], (int, float))), None)
        # A placement has a position as well as a size: a stored-image record (width_px,
        # height_px, no row) is not one. Fixed 2026-09-25 after it miscounted rio-1's image list.
        has_pos = any(k == "row" or k.endswith("_row") for k in keys) and any(k in ("col", "column") or k.endswith(("_col", "_column")) for k in keys)
        if wk and hk and has_pos and any("place" in p or "image" in p for p in path):
            placements.append({k: keys[k] for k in keys if isinstance(keys[k], (int, float))})
        if path and "cursor" in path[-1]:
            cursors.append(v)
    if isinstance(v, str) and any("repl" in p for p in path):
        replies.append(v)
    if isinstance(v, int) and path and any(t in path[-1] for t in ("row", "held", "total", "history", "lines")) \
            and not any("place" in p or "cursor" in p or "image" in p for p in path):
        history.append((".".join(path), v))


def placed_ok(p):
    vals = set(p.values())
    return {64, 19}.issubset(vals) and 0 in vals


def cursor_ok(c):
    if isinstance(c, dict):
        vals = [v for v in c.values() if isinstance(v, int)]
        return 19 in vals and 0 in vals
    if isinstance(c, list):
        return c[:2] in ([19, 0], [0, 19])
    return False


hist_ok = [h for h in history if 9839 <= h[1] <= 10241]
print(json.dumps({
    "workspace": ws.name,
    "picture_64x19_at_top_left": bool(placements) and any(placed_ok(p) for p in placements) and len(placements) == 1,
    "placements_found": placements,
    "cursor_row_19": any(cursor_ok(c) for c in cursors),
    "cursors_found": cursors,
    "kitty_ok_in_replies": any("OK" in r and "Gi=1" in r for r in replies),
    "replies_found": replies[:6],
    "history_10040_within_2pct": bool(hist_ok),
    "history_candidates": history[:8],
}, indent=1, default=str))
