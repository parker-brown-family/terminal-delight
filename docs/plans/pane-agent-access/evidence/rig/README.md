# The War Room photo rig

A hidden Terminal Delight window on its own session (`warroomrig1`), with six
stand-in agents and a saved chart, photographed with `grim -T`. Nothing appears on
screen and the live window is never touched.

- `claude` — the stand-in. It is named `claude` because a pane's mode is read
  from the process name. It holds a one-line transcript open, so Terminal Delight
  reads its size and model the way it reads a real agent's. It posts a response
  card into its pane's surface folder and prints a status line.
- `warroomrig1.toml` — the saved layout. It has four seats (one read-only), two
  agents off the chart, two projects, and four lines including a bypass with two
  prompts left. Paths were written for `$HOME/.cache/war-room-rig`.
- `rig.sh` — launches the window with `TD_WAR_ROOM_DEMO=<seat id>` (open the War
  Room; a seat id opens that seat's card), waits for the vitals sweep, and shoots.
  It exports `HYPRLAND_INSTANCE_SIGNATURE` and `WAYLAND_DISPLAY`, because an
  agent's shell has neither.

The shots taken with it are in `reports/2026-10-22-war-room-evidence/`. The stand-in
writes into `~/.claude/projects/-home-parker--cache-war-room-rig-seats-*`, which
is safe to delete afterwards.

## The copy rig (`copies/`)

The same layout, driven through a real copy ask with nothing on screen. A copy is started from the asked agent's own `/proc/<pid>/exe`, and for a shell script that is `/bin/bash`, so this rig's stand-in is compiled:

- `claude.c` — as an agent, it writes a one-line transcript, holds it open for life (the agent naming its own conversation), and runs `claude.sh` beside it. Started with `--fork-session`, it hands over to `claude-copy.sh`.
- `claude.sh` — the agent side. The release lead asks each seat named in `ASK_COPY` for a copy, through `ctl mcp from` (the relay a real agent's MCP calls take). It reads each answer back with `ask_answer`, then writes `ALL-ASKS-FINISHED`.
- `claude-copy.sh` — the copy side. It records its arguments, the names of its environment and its folder, then speaks Claude's stream-json: servers on request, `["Read"]` at init once the question arrives, an answer, and a result reporting what it read.
- `copy-rig.sh` — builds the stand-in and sets the press limit to 300k. Backend lead (290,211) runs at once; auth refactor (360,090) waits on the rail. The script opens the rail with Ctrl+Shift+N, photographs the card, presses `y` through `hl.dsp.send_shortcut`, and prints what the asker read, what each copy was started with, and the record.

Run 2026-10-22: `copy-rig-2026-10-22.out` and `asks-record-copies-2026-10-22.jsonl`. Shots: `reports/2026-10-22-war-room-evidence/war-room-copy-card.png` and `war-room-copy-after-press.png`. The stand-in copy's own answer says "done", which is why the rig waits for a marker of its own.

