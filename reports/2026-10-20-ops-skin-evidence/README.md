# OPS native review — evidence

The review page is `reports/2026-10-20-ops-skin-review.html`. Every capture in it is embedded in the page itself as WebP, so this directory holds only the rig that made them.

- `capture.sh <td-binary> <out-dir> <scene> <surface>` runs one scene in a hidden window on a special workspace. Each scene gets its own session, config, cache and state, and the rig never touches a live window. The scenes are `before`, `tactical`, `gruvbox`, `tokyo`, `current`, `warped`, `intrusive` and `chalkwet`. The surfaces are `workspace`, `tray`, `usage`, `gauges`, `rail` and `paint`. The scene list is at the top of the script.
- `run-all.sh <td-binary> <out-root>` runs every scene the review uses.
- `emitter.py` is the staged terminal content: a fictional build, not a measurement.

**Run a frozen copy.** Bash reads a script as it executes, so editing `capture.sh` while it runs corrupts the run in progress. That happened once during this pass. Copy the three files somewhere else and set `OPS_ROOT` to the checkout:

```sh
cp capture.sh run-all.sh emitter.py /some/frozen/dir/
```
```sh
cd /some/frozen/dir && OPS_ROOT=/path/to/terminal-delight bash run-all.sh /path/to/frozen/terminal-delight /tmp/ops-shots
```

The review was made from the release binary `td-ops-5a0bd02f` (the first eight hex digits of its sha256), built from the `feat/ops-skin` branch. In the Tactical scenes no skin is picked, and `skin.txt` answers `active=ops`, which proves the design asks for its skin. The `usage` surface closes the demo's panel with Escape and also writes `slot.png`. The third Escape reaches the terminal as `^[`, so the page crops that capture to the left bar.
