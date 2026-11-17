# /// script
# dependencies = ["wgpu==0.33.0"]
# ///
"""Offscreen Vulkan timing of TD's actual full-window WGSL, after pipeline warmup.
VK_DRIVER_FILES must select the same software driver as window-bench.py.
Run with uv run --script scripts/perf/shader-bench.py > samples.jsonl."""

import argparse
import array
import json
from pathlib import Path
import statistics
import time

import wgpu

WIDTH, HEIGHT = 900, 600
root = Path(__file__).resolve().parents[2]
source = root.parent / "zed-upstream/crates/gpui_wgpu/src/crt_pass.wgsl"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--shader", type=Path, default=source)
parser.add_argument(
    "--images", type=Path, help="save RGBA readbacks for pixel comparisons"
)
args = parser.parse_args()
if args.images:
    args.images.mkdir(parents=True, exist_ok=True)
adapter = wgpu.gpu.request_adapter_sync(power_preference="low-power")
if adapter.info["adapter_type"] != "CPU":
    raise RuntimeError(f"expected isolated software renderer: {adapter.info}")
dev = adapter.request_device_sync()
shader = dev.create_shader_module(code=args.shader.read_text())
pipeline = dev.create_render_pipeline(
    layout="auto",
    vertex={"module": shader, "entry_point": "vs_crt", "buffers": []},
    fragment={
        "module": shader,
        "entry_point": "fs_crt",
        "targets": [{"format": "rgba8unorm"}],
    },
)
usage = wgpu.TextureUsage
src = dev.create_texture(
    size=(WIDTH, HEIGHT, 1),
    format="rgba8unorm",
    usage=usage.TEXTURE_BINDING | usage.COPY_DST,
)
dst = dev.create_texture(
    size=(WIDTH, HEIGHT, 1),
    format="rgba8unorm",
    usage=usage.RENDER_ATTACHMENT | usage.COPY_SRC,
)
# A pattern that varies along both axes, so a readback (`--images`) can
# expose a blur that moves its samples vertically as well as horizontally.
source = bytes(
    (x * 43 ^ y * 29 ^ (x + y) // 4) & 255
    for y in range(HEIGHT)
    for x in range(WIDTH * 4)
)
dev.queue.write_texture(
    {"texture": src}, source, {"bytes_per_row": WIDTH * 4}, (WIDTH, HEIGHT, 1)
)
uniform = dev.create_buffer(
    size=3120, usage=wgpu.BufferUsage.UNIFORM | wgpu.BufferUsage.COPY_DST
)
group = dev.create_bind_group(
    layout=pipeline.get_bind_group_layout(0),
    entries=[
        {"binding": 0, "resource": {"buffer": uniform}},
        {"binding": 1, "resource": src.create_view()},
        {
            "binding": 2,
            "resource": dev.create_sampler(min_filter="linear", mag_filter="linear"),
        },
    ],
)
view = dst.create_view()


def render(case):
    # Offsets match the production uniform buffer in wgpu_renderer.rs.
    p = [0.0] * 780
    if case in ["warp", "glow", "tube_blur"]:
        p[2] = 1
        p[4:8] = [140, 50, 740, 530]
        if case == "warp":
            p[132:135] = [0.35, 0.14, 0.06]
        elif case == "glow":
            p[396] = 0.7
        else:
            p[397] = 7
    if case == "frost":
        p[260:268] = [150, 100, 600, 400, 7, 12, 0.4, 8]
    if case == "grade":
        p[3] = 1
        p[524:528] = [140, 50, 740, 530]
        p[652:654] = [1.25, 0.8]
    dev.queue.write_buffer(uniform, 0, array.array("f", p))
    t = time.perf_counter()
    enc = dev.create_command_encoder()
    for _ in range(8):
        render_pass = enc.begin_render_pass(
            color_attachments=[
                {
                    "view": view,
                    "load_op": "clear",
                    "clear_value": (0, 0, 0, 1),
                    "store_op": "store",
                }
            ]
        )
        if case != "clear":
            render_pass.set_pipeline(pipeline)
            render_pass.set_bind_group(0, group)
            render_pass.draw(3)
        render_pass.end()
    dev.queue.submit([enc.finish()])
    dev.queue.on_submitted_work_done_sync()
    return 1000 * (time.perf_counter() - t) / 8


cases = ["clear", "identity", "warp", "glow", "tube_blur", "frost", "grade"]
times = {c: [] for c in cases}
for case in cases:
    render(case)
    if args.images:
        pixels = dev.queue.read_texture(
            {"texture": dst}, {"bytes_per_row": WIDTH * 4}, (WIDTH, HEIGHT, 1)
        )
        (args.images / f"{case}.rgba").write_bytes(pixels)
for repeat in range(5):
    for case in cases[:: 1 if repeat % 2 == 0 else -1]:
        times[case].append(render(case))
for case, values in times.items():
    print(
        json.dumps(
            dict(
                case=case,
                adapter=adapter.info["device"],
                ms=values,
                median=statistics.median(values),
                minimum=min(values),
                maximum=max(values),
            )
        )
    )
