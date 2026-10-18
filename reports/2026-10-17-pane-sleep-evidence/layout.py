"""Same single visible quiet pane and thirty hidden agent fixtures per build."""
import json
import sys
from pathlib import Path
session, out, script, switch, counters = sys.argv[1:]
lines = ['active = 0', 'agent_tint = false', 'anchor_top = false', 'left_bar = true',
         'scale = 0.85', 'rail_pinned = false', '[mcp]', 'enabled = true',
         'expose = "all"', 'writable = true', '[theme]', 'id = "custom"', 'syntax = true',
         'syntax_scheme = "agentic"', 'palette = "terminal-delight"', '[theme.grade]', 'crt = false']
for number in range(31):
    command = "exec sleep 1200" if number == 0 else f"python3 {script} {number} {switch} {counters}"
    lines += ['[[tabs]]', '[tabs.node.Leaf]', 'cwd = "/tmp"',
              f'pane_id = {number + 1}', f'name = "fixture {number}"',
              'resume = ' + json.dumps(command), '[tabs.node.Leaf.appearance]',
              'pins = ["crt"]', '[tabs.node.Leaf.appearance.grade]', 'crt = false']
Path(out).write_text("\n".join(lines) + "\n")
