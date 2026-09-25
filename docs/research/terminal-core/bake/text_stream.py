"""A deterministic stream of ordinary terminal text, for speed and memory runs.

    python3 text_stream.py OUT.bin [megabytes]

Twenty panes spend nearly all their time on text, not pictures, so the performance runs
feed this instead of a picture recording: lines of 60 to 118 columns with a colour or
attribute change every few words, occasional wide CJK characters, combining marks and
emoji, and a progress line redrawn with a carriage return now and then. Seeded, so every
core sees the same bytes. At the default 8 MB it is about 90,000 lines, well past the
10,000 lines of scrollback every harness keeps.
"""
import random
import sys

out = sys.argv[1]
target = int(float(sys.argv[2]) if len(sys.argv) > 2 else 8) * 1024 * 1024
rng = random.Random(20260925)
WORDS = ("cargo build release warning unused variable test passed failed thread main panicked "
         "compiling finished target debug info error src lib rs mod fn impl struct enum match").split()
CJK = "日本語の文字列漢字表示確認"
MARKS = ["é", "ä", "ñ", "ô"]
EMOJI = ["\U0001F680", "✅", "❌", "\U0001F4E6"]
SGR = ["\x1b[31m", "\x1b[32m", "\x1b[33m", "\x1b[34m", "\x1b[1m", "\x1b[3m", "\x1b[38;5;208m", "\x1b[38;2;120;200;255m", "\x1b[0m"]

buf = bytearray()
n = 0
while len(buf) < target:
    n += 1
    width = rng.randint(60, 118)
    line, cols = [], 0
    while cols < width:
        r = rng.random()
        if r < 0.12:
            tok = rng.choice(SGR)
            line.append(tok)
            continue
        if r < 0.15:
            tok = rng.choice(CJK) * rng.randint(1, 3)
            cols += 2 * len(tok)
        elif r < 0.17:
            tok = rng.choice(MARKS)
            cols += 1
        elif r < 0.18:
            tok = rng.choice(EMOJI)
            cols += 2
        else:
            tok = rng.choice(WORDS)
            cols += len(tok)
        line.append(tok + " ")
        cols += 1
    buf += ("".join(line) + "\x1b[0m\r\n").encode()
    if n % 400 == 0:
        for p in range(0, 101, 20):
            buf += f"\r\x1b[32m[{'#' * (p // 5):<20}] {p:3d}%\x1b[0m".encode()
        buf += b"\r\n"
open(out, "wb").write(bytes(buf))
print(f"{len(buf):,} bytes, {n:,} lines -> {out}")
