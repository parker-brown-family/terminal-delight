"""Deterministic hosted agent-shaped output, never a real agent session."""
import sys
import time
from pathlib import Path

number, switch, counters = sys.argv[1:]
counter = Path(counters) / number
count = 0
counter.write_text('0')
temporary = counter.with_name('.' + number)
print("fixture-ready " + number, flush=True)
while True:
    if Path(switch).exists():
        print("still thinking (esc to interrupt) " + str(count), flush=True)
        count += 1
        temporary.write_text(str(count))
        temporary.replace(counter)
    time.sleep(0.05)
