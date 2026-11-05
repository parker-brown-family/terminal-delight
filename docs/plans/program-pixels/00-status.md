# Status: Program pixels — pictures that programs draw

**Difficulty: 8/10** — cursor-placed pictures need either TD's own read loop, replacing alacritty's in all three places it runs (session host, the window's copy, window-owned panes), or a swap of the emulator core itself. Either way the change sits under every byte of every pane; a mistake shows up as a pane that stalls, answers twice or drifts from its host, which no unit test of the picture code would see; and the memory cost of pictures multiplies across twenty panes → **four gates**, with the choice of road decided by measurement before anything draws.
**Turned out to be:** not yet known.

- Gate 1 — Product: **WAITING** on Parker's notes on `reports/2026-09-24-program-pixels.html`, the research brief (revised 2026-09-25), which ends on five questions (which program draws first, **own the loop or swap the core**, identity answers, a window-wide picture budget, video now or later). As with the documents-in-the-pane plan, his notes and concurs on that page are the approval channel; `01-product.md` gets written from them.
- Gate 2 — Architecture: not started. Carries the road the head-to-head test picks, the picture store and its quota, host-to-window forwarding of file and shared-memory pictures, and the file-path rules (`d-files` modal). If the road is the loop, the seven invariants in the `d-invariants` modal are its checklist.
- Gate 3 — Program Design: not started.
- Gate 4 — Slice plan: not started. The brief's figure 09 is the proposed order: identity answers → **choose the road** (rio-vt and libghostty-vt fed the recorded programs, TD's alacritty surface counted; the gate) → ratatui-image draws → pictures at the cursor → upstream recognition; video later.

## Revision, 2026-09-25

The first version of the brief (commit 45567b7) weighed three routes — tee scanner, own the loop, patch vte + alacritty_terminal — and recommended owning the loop. It missed option F of TD's own foundation review, `docs/2026-08-29-foundation-interrogation-zed-gpui-quickshell.md`: swap the VT seam for rio-vt or libghostty-vt, "when image support becomes a committed TD feature". Parker's links to wterm (whose `@wterm/ghostty` core embeds libghostty 1.3.1 and draws Kitty pictures) and asciinema prompted the correction. The brief now carries a section and figure 07 on the swap, question 2 asks loop or swap, and step 1 is a head-to-head test instead of a loop spike. asciinema recordings are named as a separate, smaller track (a new floating-square document type), not part of this plan.

## What this plan starts from (2026-09-24)

- **The floating square** (documents-in-the-pane, all ten slices merged): Alt+click decodes a picture with gpui's loader, holds the only copy, draws it inside the pane under the glass, and gives the texture back to every window's atlas; `scripts/doc-float-soak.sh` proves the give-back on the real GPU. The picture painter here reuses that give-back.
- **The size answers** (slice 9 of that plan): `CSI 14 t` from live geometry, `CSI 16 t` from a scanner in the host's tee (`ptyscan.rs`), device pixels to the pseudoterminal.
- **The morning's research**: `docs/research/gui-in-the-tui/graphics-protocols.md` on the local branch `research/gui-in-the-tui` (commits b26b67a, 4a2a6d1), not meant to merge because it quotes outside source; cite it by commit. Its routes A/B/C are this brief's "scanner / own the loop / patch the parser".

## Measured tonight (evidence in `evidence/`, scripts in `spikes/`)

- In a real TD pane, `kitten icat` waits 10.1 s and exits with "Timed out waiting for a response from the terminal", because TD's device-attributes reply `ESC[?6c` is too short for icat's check. A defect today, independent of pictures.
- ratatui-image and viu ask with Kitty queries and fall back to half-blocks; chafa never asks and reads `TERM=alacritty`; mpv is told by a flag. Of chafa, timg and ratatui-image, only ratatui-image uses Unicode placeholders.
- A hosted pane takes in video-sized Kitty frames at 110–149 MB/s (discarded today), against mpv's 97 MB/s at 1200×675 and 30 fps; the window's copy was never dropped in the runs that watched for it.

## Open follow-ups (2026-09-24)

- #750 — `kitten icat` hangs ten seconds in every pane because the device-attributes reply is too short for it (step 0 of the build closes it).
- #751 — two comments on the pane read path describe code that no longer exists (`host.rs:59-61`, `socketpty.rs:23-29`).

## Notes for a fresh session

- The brief and this folder are in PR #752. Worktree `~/Work/td-program-pixels`, branch `plan/program-pixels`, cut from main at 81ec545.
- The brief is assembled: edit `reports/_program_pixels_body.html` and `reports/_program_pixels.css`, then run `python3 reports/_assemble_program_pixels.py`. The notes system is read from the decision-brief skill at build time.
- The narrative gate was run on the draft and the final: arc `journey`, opens cold 0.71.
- Parker's annotated copy will come back as a download; read its notes with the same island parser used for the morning brief.
