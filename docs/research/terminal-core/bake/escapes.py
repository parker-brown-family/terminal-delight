"""List the escape sequences in a raw capture, with APC payloads elided."""
import re, sys
data = open(sys.argv[1], "rb").read()
limit = int(sys.argv[2]) if len(sys.argv) > 2 else 60
out = []
for m in re.finditer(rb"\x1b_G([^;\x1b]*)(;[^\x1b]*)?\x1b\\|\x1b\[[0-9;?<>=]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)|\x1b[78=>]", data):
    s = m.group(0)
    if s.startswith(b"\x1b_G"):
        ctrl = m.group(1).decode()
        if ctrl.startswith("m=") and len(ctrl) <= 5:
            if out and out[-1].startswith("  [chunks"):
                n = int(out[-1].split()[1].rstrip("]")) + 1
                out[-1] = f"  [chunks {n}]"
            else:
                out.append("  [chunks 1]")
            continue
        out.append(f"APC G {ctrl} payload={len(m.group(2) or b'') - 1 if m.group(2) else 0}")
    else:
        out.append(repr(s.decode('latin1')))
for line in out[:limit]:
    print(line)
print(f"... {len(out)} sequences")
