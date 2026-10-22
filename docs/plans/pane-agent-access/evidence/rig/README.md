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
