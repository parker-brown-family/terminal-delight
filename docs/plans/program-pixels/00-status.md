# Status: Program pixels — pictures that programs draw

**Difficulty: 8/10** — the change that makes cursor-placed pictures possible replaces the read loop every pane's output goes through, in all three places it runs (session host, the window's copy, window-owned panes). A mistake there shows up as a pane that stalls, answers twice or drifts from its host, which no unit test of the picture code would see; and the memory cost of pictures multiplies across twenty panes → **four gates**, with a spike of the loop as the first thing built.
**Turned out to be:** not yet known.

- Gate 1 — Product: **WAITING** on Parker's notes on `reports/2026-09-24-program-pixels.html`, the research brief, which ends on five questions (which program draws first, loop or parser patch, identity answers, a window-wide picture budget, video now or later). As with the documents-in-the-pane plan, his notes and concurs on that page are the approval channel; `01-product.md` gets written from them.
- Gate 2 — Architecture: not started. Carries the loop design (the seven invariants in the brief's modal), the picture store and its quota, host-to-window forwarding of file and shared-memory pictures, and the file-path rules (`d-files` modal).
- Gate 3 — Program Design: not started.
- Gate 4 — Slice plan: not started. The brief's figure 08 is the proposed order: identity answers → loop spike on the host (the gate) → ratatui-image draws → pictures at the cursor → upstream recognition; video later.

## What this plan starts from (2026-09-24)

- **The floating square** (documents-in-the-pane, all ten slices merged): Alt+click decodes a picture with gpui's loader, holds the only copy, draws it inside the pane under the glass, and gives the texture back to every window's atlas; `scripts/doc-float-soak.sh` proves the give-back on the real GPU. The picture painter here reuses that give-back.
- **The size answers** (slice 9 of that plan): `CSI 14 t` from live geometry, `CSI 16 t` from a scanner in the host's tee (`ptyscan.rs`), device pixels to the pseudoterminal.
- **The morning's research**: `docs/research/gui-in-the-tui/graphics-protocols.md` on the local branch `research/gui-in-the-tui` (commits b26b67a, 4a2a6d1), not meant to merge because it quotes outside source; cite it by commit. Its routes A/B/C are this brief's "scanner / own the loop / patch the parser".

## Measured tonight (evidence in `evidence/`, scripts in `spikes/`)

- In a real TD pane, `kitten icat` waits 10.1 s and exits with "Timed out waiting for a response from the terminal", because TD's device-attributes reply `ESC[?6c` is too short for icat's check. A defect today, independent of pictures.
- ratatui-image and viu ask with Kitty queries and fall back to half-blocks; chafa never asks and reads `TERM=alacritty`; mpv is told by a flag. Of chafa, timg and ratatui-image, only ratatui-image uses Unicode placeholders.
- A hosted pane takes in video-sized Kitty frames at 110–149 MB/s (discarded today), against mpv's 97 MB/s at 1200×675 and 30 fps; the window's copy was never dropped in the runs that watched for it.

## Notes for a fresh session

- Worktree `~/Work/td-program-pixels`, branch `plan/program-pixels`, cut from main at 81ec545.
- The brief is assembled: edit `reports/_program_pixels_body.html` and `reports/_program_pixels.css`, then run `python3 reports/_assemble_program_pixels.py`. The notes system is read from the decision-brief skill at build time.
- The narrative gate was run on the draft and the final: arc `journey`, opens cold 0.71.
- Parker's annotated copy will come back as a download; read its notes with the same island parser used for the morning brief.
