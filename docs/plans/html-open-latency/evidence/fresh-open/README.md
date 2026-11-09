# Evidence: fresh opens lose Chromium's hidden work

Measured 2026-11-07 on the Legion (16 cores), Chromium 152.0.7977.82, against the installed `td-f32edeb3-wave` build (`--build-info`: opt-level 3, clean, sha f32edeb3). The change is PR 1391, merged into `dev/0.5.1` as `1d93d991`.

## Native, in the real window

`native.py` is the isolated Xvfb rig from `../native/native.py`, with `PROFILE_FRESH_FILES` declaring a new file path every cycle, so every open misses the cache while the browser stays warm. `run.sh` alternates the installed build and the branch build per brief and round; `summ.py` reports medians.

- `native-quiet-final.txt`: the final branch build, load average 5 to 8, 27 warm opens per cell. 200 KB brief 673 → 486 ms median (p90 791 → 548); 1.4 MB brief 842 → 595 ms (p90 1006 → 652).
- `native-quiet/<build>-<brief>/rN-samples.json` and `rN-memory.jsonl`: the raw samples and the rig's memory snapshots behind those medians. Chromium PSS with the reader open 288 → 223 MiB, closed and idle 258 → 229 MiB; 8 → 5 browser processes.
- `native-pooled-under-load.json`: three earlier runs at load averages 20 to 60, which still included the ready page. Pooled medians 1328 → 843 ms and 1610 → 1003 ms. Kept as the under-load picture; the ready page is no longer in the merged code.

## Engine only

`engine-ready-vs-not.jsonl`: the snapshot engine alone (open plus first screen) on five real briefs, interleaved, load average 11. Installed engine 469 ms median at 304 MiB idle over 8 processes; with a ready page 355 ms at 283 MiB over 6; without it 416 ms at 236 MiB over 5. This is the measurement behind keeping the ready page out.

## The trace finding

A Chromium trace of one fresh open (not kept: 24 MB) showed three renderers for one page. One loaded `chrome://omnibox-popup.top-chrome/omnibox_popup_aim.html` with lit and mojo bindings, about 190 ms of main-thread script per open; one was Chromium's idle spare. With `--disable-features=PreloadTopChromeWebUI,WebUIOmniboxAimPopup,...` a second trace had no omnibox mention at all. A trace of a clipped first-screen capture showed four `Display::DrawAndSwap` passes (34, 25, 34, 78 ms) before `CopyOutputResultSender::SendResult`.

## Review

`round1.md` to `round4.md`: four read-only Codex rounds over the branch. Seven real findings were fixed in their own commits; round 4 found no confirmed regression and one inferred edge, also fixed.

## Tests

- `the_first_screen_taken_early_is_the_picture_a_clipped_capture_takes` (app/tests/snapshot_engine.rs): every pixel of the unclipped first screen against the clipped capture, at scales 1, 1.25, 1.6 and 2.
- Engine suite 94/94, docview unit tests 215/215, full app suite 3255/3256 with the known load flake `a_pane_knows_which_session_and_pane_it_is` (issue 1379; passes alone 4 of 4).
- `the_pages_own_notes_ui_is_not_in_the_picture` failed 2 runs in 6 midway through the branch, which exposed two captures overlapping on one page; after the fix it passed 12 of 12.
