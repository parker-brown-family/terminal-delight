"""Print what a program writes after its last Kitty APC sequence."""
import sys
for path in sys.argv[1:]:
    data = open(path, "rb").read()
    i = data.rfind(b"\x1b_G")
    end = data.find(b"\x1b\\", i) + 2
    print(path, "control of last APC:", data[i:i + 80].split(b";")[0])
    print("   after:", repr(data[end:end + 120]))
