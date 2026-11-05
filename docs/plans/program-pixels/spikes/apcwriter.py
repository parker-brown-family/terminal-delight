#!/usr/bin/env python3
"""Write video-sized Kitty frames to the terminal as fast as it drains them.

Each frame is what mpv --vo=kitty sent at a 1200x675 pane in the capture:
raw RGB (f=24), base64, in 4096-byte chunks, the first carrying the header.
The terminal throws the pixels away (TD's parser drops APC today), so this
measures only how fast the pane's read path takes the bytes in.

    apcwriter.py FRAMES OUTFILE [apc|text]
"""
import base64, json, os, sys, time

frames, out = int(sys.argv[1]), sys.argv[2]
mode = sys.argv[3] if len(sys.argv) > 3 else "apc"
W, H = 1200, 675
b = base64.b64encode(os.urandom(W * H * 3))
chunks = [b[i:i + 4096] for i in range(0, len(b), 4096)]
if mode == "apc":
    parts = [b"\x1b_Ga=T,f=24,s=%d,v=%d,C=1,q=2,m=1;" % (W, H) + chunks[0] + b"\x1b\\"]
    parts += [b"\x1b_Gm=%d;" % (0 if i == len(chunks) - 1 else 1) + c + b"\x1b\\"
              for i, c in enumerate(chunks[1:], 1)]
    frame = b"\x1b[1;1H" + b"".join(parts)
else:
    # the same number of bytes as printable text, 100 columns a line
    frame = b"\x1b[1;1H" + b"".join(b[i:i + 100] + b"\r\n" for i in range(0, len(b), 100))
time.sleep(float(os.environ.get("APC_DELAY", "0")))
fd = sys.stdout.fileno()
per = []
t0 = time.monotonic()
for _ in range(frames):
    t = time.monotonic()
    view = memoryview(frame)
    while view:
        n = os.write(fd, view)
        view = view[n:]
    per.append(time.monotonic() - t)
total = time.monotonic() - t0
json.dump({"mode": mode, "frames": frames, "frame_bytes": len(frame),
           "secs": round(total, 3), "mb_per_s": round(frames * len(frame) / total / 1e6, 1),
           "fps": round(frames / total, 1),
           "frame_ms_median": round(sorted(per)[len(per) // 2] * 1000, 1),
           "frame_ms_max": round(max(per) * 1000, 1)}, open(out, "w"))
