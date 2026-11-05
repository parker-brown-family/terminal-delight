#!/usr/bin/env python3
"""Run the source-backed renderer profile without taking desktop input."""
import argparse
import json
import os
from pathlib import Path
import subprocess

p = argparse.ArgumentParser()
p.add_argument("--gpu", choices=["default", "vulkan"], default="default")
p.add_argument("--out", type=Path, required=True)
p.add_argument("files", nargs="+", type=Path)
a = p.parse_args()
root = Path(__file__).resolve().parent
subprocess.run(["cargo", "build", "--release", "--offline"], cwd=root, check=True)
env = os.environ.copy()
env.pop("PROFILE_GPU", None)
if a.gpu == "vulkan":
    env["PROFILE_GPU"] = "vulkan"
a.out.parent.mkdir(parents=True, exist_ok=True)
with a.out.open("w") as output:
    subprocess.run([str(root / "target/release/td-html-open-profile"),
                    *[str(f.resolve(strict=True)) for f in a.files]],
                   env=env, stdout=output, check=True)
rows = [json.loads(s) for s in a.out.read_text().splitlines()]
print(json.dumps({"output": str(a.out), "records": len(rows), "gpu": a.gpu}))
