# Program pixels — the scripts behind the research brief

Everything the brief `reports/2026-09-24-program-pixels.html` measured came from
these, run on 2026-09-24 against the installed build `td-81ec545-rows-where-drawn`
(main at 81ec545). Nothing here is part of TD.

| Script | What it answers |
|---|---|
| `fakepty.py IDENTITY OUT.json -- cmd…` | What an image program asks a terminal and sends back, under a stand-in that answers as TD does today (`td-today`), as a TD that says yes to pictures (`td-kitty`), or as kitty (`kitty`). Records every sequence; draws nothing. |
| `tdrun.sh OUTDIR 'cmd' …` | Runs each command in its own tab of a real, hidden TD window and records exit code, wall time and stderr. |
| `apcprobe.sh OUTDIR FRAMES [hosted\|owned]` | How fast a TD pane takes in video-sized Kitty frames (`apcwriter.py`) and the same bytes as text, and whether the window's copy was dropped and re-attached (it diffs the window's socket inodes). |
| `ratatui-image-probe/` | The smallest ratatui-image program: detect, draw one picture full-screen, log the protocol it chose. |

The hidden window is the recipe from `scripts/doc-float-soak.sh`: its own
`TD_SESSION`, a special workspace, `render_unfocused`, `no_initial_focus`, and
killed at once if it lands anywhere visible.

The programs were fetched, not installed: kitty's static `kitten` release binary
(0.49.1), chafa 1.18.2 unpacked from the Arch package (run with
`LD_LIBRARY_PATH` pointing at its `usr/lib`), viu 1.6.1 through
`cargo install --root`, and mpv 0.41.0 from the system. The kitten binary is
GPL-3.0 and is deliberately not in the repository.

Raw output is in `../evidence/`: `stand-in/` per program and identity,
`real-td/` per command, and `throughput/runs.csv` with every run, including the
first hosted run, which did not yet watch for re-attaches.
