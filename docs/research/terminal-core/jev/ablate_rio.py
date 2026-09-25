"""Why does core-fit v2 still lean does_not on rio-vt's placements_readable (0.45)?

Two suspects, tested apart on four copies of the rio-vt item:
  A  control, exactly what the run sent
  B  claim-check verdicts removed (does a quote-level 'unsupported' read as 'false'?)
  C  the observed line also names the pixels, read from rio-vt 0.5.28's source:
     graphics.kitty_images[id].data.pixels is public (rio-graphics GraphicData.pixels)
  D  both
    python3 ablate_rio.py && jds run specs/core-fit/core-fit.jds.json --items items/rio-ablation.json --out runs/core-fit-rio-ablation
"""
import copy
import json
from pathlib import Path

HERE = Path(__file__).parent
rio = next(c for c in json.loads((HERE / "items" / "cores.json").read_text()) if c["id"] == "rio-vt")
PIXELS = " Each image's decoded pixels are public too: graphics.kitty_images maps the image id to a StoredImage whose data carries width, height, colour type and pixels (read from the rio-vt 0.5.28 and rio-graphics 0.5.28 source)."


def variant(tag, strip, pixels):
    v = copy.deepcopy(rio)
    v["id"] = f"rio-vt/{tag}"
    if strip:
        for e in v["evidence"].values():
            e["check"] = "supported"
    if pixels:
        v["measured"] = v["measured"] + PIXELS
    return v


items = [variant("A-control", False, False), variant("B-no-verdicts", True, False),
         variant("C-pixels", False, True), variant("D-both", True, True)]
(HERE / "items" / "rio-ablation.json").write_text(json.dumps(items, indent=1))
print(len(items), "variants")
