# Probes behind the Gate 1 brief (2026-10-26)

Read-only measurement scripts and the logs of the disposable-copy experiments. Run from a shell, never inside a Terminal Delight pane you care about.

- `agent_mem.py` — per-agent process tree: PSS and SwapPss of the agent and every descendant.
- `sys_mem.py`, `attr.py` — the whole machine by group, and helpers attributed to the agent that owns them.
- `idle_touch.py <seconds> <pid…>` — CPU, minor and major faults of idle agents over a window.
- `probe4.sh` — the safe wake-latency probe: a `--fork-session` copy in tmux, its pid taken from its own tmux pane and checked before anything is signalled. Controls C0–C4, rest C5–C6.
- `codexprobe2.sh` — does Codex's shared service end a window's helpers after the window exits? Filter the new children by cwd before drawing a conclusion; other windows start sets in the same seconds.
- `probe2.log`, `probe3.log`, `probe4.log` — results. probe2's log has two lines removed that quoted another session's transcript.
- probe3 is not kept: its lookup became a wildcard over every live Claude when the id it read was empty, and its last step sent SIGTERM to what it found. A script that can stop a live agent does not belong in a repository, whatever it is named. Its 28 s and 30 s wake readings are void (the matcher missed the U+00A0 after `❯`).
- `live-rig.sh <binary> <out>` — the off-screen end-to-end check: a hidden window on a throwaway session and config, a real Claude on a one-Haiku-call conversation, rest → sleep → wake by `send_shortcut` → rest again.
- `live-rig-1.log`, `live-rig-2.log` — two runs of `live-rig.sh`, 2026-10-26. Run 1: adopted in 5 s, rested at 78 s, asleep 63 s later (agent gone, wake file written), woke 13 s after a key, back on its own conversation, host untouched. Run 2, after the wake script learned to wait for its group: the woken agent was born inside it (295 MB charged against 266 MB PSS), and its second rest took PSS from 267 MB to 84 MB with 176 MB in swap. Screenshots were attempted with `grim -T` and timed out on the hidden window both times.
