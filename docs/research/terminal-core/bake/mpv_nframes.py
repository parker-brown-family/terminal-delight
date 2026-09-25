"""Cut the mpv capture after N frames to see when rio-vt's placement disappears."""
data = open("mpv.bin", "rb").read()
pos = 0
for n in range(1, 5):
    i = data.find(b"\x1b_Gm=0;", pos)
    end = data.find(b"\x1b\\", i) + 2
    pos = end
    if n in (2, 3, 4):
        open(f"mpv-{n}frames.bin", "wb").write(data[:end])
# the 4-frame stream with only the trailing cursor-show added back
open("mpv-4frames-show.bin", "wb").write(data[:pos] + b"\x1b[?25h")
open("mpv-4frames-mouseoff.bin", "wb").write(data[:pos] + b"\x1b[?1003l")
print("ok")
